//! PDF signing (task 3A.8): a detached CAdES signature (PAdES baseline B-B: signed attributes
//! with content type, message digest and ESS signing-certificate-v2) written as an incremental
//! update with an invisible signature field. The key and certificate are PEM files chosen by
//! the user (PKCS#8 RSA key); nothing is overwritten, the result is `<name> (signed).pdf`.
//!
//! The input is first rewritten by qpdf without object streams so the file has a classic
//! cross-reference table; the update is then appended in place. Documents that already have an
//! `/AcroForm` or a non-inline `/Annots` on page one are refused rather than risk damaging them.

use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
use const_oid::ObjectIdentifier;
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Decode, DecodePem, Encode};
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;
use std::path::Path;
use std::process::Command;
use x509_cert::Certificate;
use x509_cert::attr::Attribute;

const SIG_BYTES: usize = 8192;
const OID_SIGNING_CERT_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");

fn qpdf_text(qpdf: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new(qpdf)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() && o.status.code() != Some(3) {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn number_after(hay: &str, key: &str) -> Option<u64> {
    let i = hay.find(key)? + key.len();
    hay[i..]
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Appends `extra` just before the final `>>` of a dictionary's text.
fn add_to_dict(dict: &str, extra: &str) -> Result<String, String> {
    let end = dict.rfind(">>").ok_or("unexpected object syntax")?;
    Ok(format!("{} {extra} {}", &dict[..end], &dict[end..]))
}

/// The CMS signature (DER) over `signed` (the two byte ranges of the PDF).
fn cms_signature(key_pem: &str, cert_pem: &str, signed: &[u8]) -> Result<Vec<u8>, String> {
    let key = RsaPrivateKey::from_pkcs8_pem(key_pem)
        .map_err(|_| "the key must be a PKCS#8 RSA private key in PEM form".to_string())?;
    let cert = Certificate::from_pem(cert_pem).map_err(|e| format!("bad certificate: {e}"))?;
    let signer = SigningKey::<Sha256>::new(key);
    let digest = Sha256::digest(signed);
    let sha256 = AlgorithmIdentifierOwned {
        oid: const_oid::db::rfc5912::ID_SHA_256,
        parameters: None,
    };
    let eci = EncapsulatedContentInfo {
        econtent_type: const_oid::db::rfc5911::ID_DATA,
        econtent: None,
    };
    let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: cert.tbs_certificate.issuer.clone(),
        serial_number: cert.tbs_certificate.serial_number.clone(),
    });
    let mut sib = SignerInfoBuilder::new(&signer, sid, sha256.clone(), &eci, Some(&digest))
        .map_err(|e| e.to_string())?;
    // ESS signing-certificate-v2 binds the signature to this certificate (PAdES B-B).
    let cert_hash = Sha256::digest(cert.to_der().map_err(|e| e.to_string())?);
    let mut v2 = vec![0x30, 0x26, 0x30, 0x24, 0x30, 0x22, 0x04, 0x20];
    v2.extend_from_slice(&cert_hash);
    let mut values = SetOfVec::new();
    values
        .insert(Any::from_der(&v2).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    sib.add_signed_attribute(Attribute {
        oid: OID_SIGNING_CERT_V2,
        values,
    })
    .map_err(|e| e.to_string())?;
    let mut b = SignedDataBuilder::new(&eci);
    b.add_digest_algorithm(sha256).map_err(|e| e.to_string())?;
    b.add_certificate(CertificateChoices::Certificate(cert))
        .map_err(|e| e.to_string())?;
    b.add_signer_info::<_, rsa::pkcs1v15::Signature>(sib)
        .map_err(|e| e.to_string())?;
    let ci: ContentInfo = b.build().map_err(|e| e.to_string())?;
    let _ = OctetString::new(Vec::<u8>::new());
    ci.to_der().map_err(|e| e.to_string())
}

/// Signs `input` and writes `out` (a path that does not exist yet).
pub fn sign_pdf(
    qpdf: &str,
    input: &Path,
    out: &Path,
    key_pem: &str,
    cert_pem: &str,
    signer_name: &str,
) -> Result<(), String> {
    let tmp = out.with_extension("kbtmp.pdf");
    let r = sign_inner(qpdf, input, &tmp, key_pem, cert_pem, signer_name);
    match r {
        Ok(bytes) => {
            let _ = std::fs::remove_file(&tmp);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(out)
                .map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut f, &bytes).map_err(|e| e.to_string())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn sign_inner(
    qpdf: &str,
    input: &Path,
    tmp: &Path,
    key_pem: &str,
    cert_pem: &str,
    signer_name: &str,
) -> Result<Vec<u8>, String> {
    let st = Command::new(qpdf)
        .arg("--object-streams=disable")
        .arg(input)
        .arg(tmp)
        .status()
        .map_err(|e| format!("qpdf not available: {e}"))?;
    if !st.success() && st.code() != Some(3) {
        return Err("could not prepare the document".into());
    }
    let tmp_s = tmp.display().to_string();
    let mut pdf = std::fs::read(tmp).map_err(|e| e.to_string())?;

    let sx = find_last(&pdf, b"startxref").ok_or("no startxref")?;
    let prev: u64 = String::from_utf8_lossy(&pdf[sx + 9..])
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or("bad startxref")?;
    let tr = find_last(&pdf, b"trailer").ok_or("no trailer")?;
    let trailer = String::from_utf8_lossy(&pdf[tr..sx]).into_owned();
    let size = number_after(&trailer, "/Size").ok_or("no /Size")?;
    let root = number_after(&trailer, "/Root").ok_or("no /Root")?;

    let root_obj = qpdf_text(qpdf, &[&tmp_s, &format!("--show-object={root}")])?;
    if root_obj.contains("/AcroForm") {
        return Err("documents that already have a form are not supported yet".into());
    }
    let pages = qpdf_text(qpdf, &[&tmp_s, "--show-pages"])?;
    let page = number_after(&pages, "page 1:").ok_or("could not find the first page")?;
    let page_obj = qpdf_text(qpdf, &[&tmp_s, &format!("--show-object={page}")])?;
    let (sig_n, wid_n) = (size, size + 1);
    let new_page = if page_obj.contains("/Annots") {
        let i = page_obj.find("/Annots").unwrap_or(0);
        let rest = page_obj[i + 7..].trim_start();
        if !rest.starts_with('[') {
            return Err("the first page has indirect annotations; not supported yet".into());
        }
        let open = page_obj
            .find('[')
            .map(|_| page_obj[i..].find('[').unwrap_or(0) + i);
        let open = open.ok_or("bad /Annots")?;
        format!(
            "{}{wid_n} 0 R {}",
            &page_obj[..=open],
            &page_obj[open + 1..]
        )
    } else {
        add_to_dict(page_obj.trim_end(), &format!("/Annots [{wid_n} 0 R]"))?
    };
    let new_root = add_to_dict(
        root_obj.trim_end(),
        &format!("/AcroForm << /Fields [{wid_n} 0 R] /SigFlags 3 >>"),
    )?;

    if !pdf.ends_with(b"\n") {
        pdf.push(b'\n');
    }
    let mut offsets: Vec<(u64, usize)> = Vec::new();
    let mut push_obj = |pdf: &mut Vec<u8>, n: u64, body: &str| {
        offsets.push((n, pdf.len()));
        pdf.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    push_obj(&mut pdf, root, &new_root);
    push_obj(&mut pdf, page, &new_page);
    let esc = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    };
    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (days, rem) = (secs / 86400, secs % 86400);
        // Civil date from days since 1970-01-01.
        let z = days as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z",
            rem / 3600,
            rem % 3600 / 60,
            rem % 60
        )
    };
    let sig_start = pdf.len();
    let sig_obj = format!(
        "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /ETSI.CAdES.detached /ByteRange [0 0000000000 0000000000 0000000000] /Contents <{}> /M ({now}) /Name ({}) >>",
        "0".repeat(SIG_BYTES * 2),
        esc(signer_name)
    );
    push_obj(&mut pdf, sig_n, &sig_obj);
    push_obj(
        &mut pdf,
        wid_n,
        &format!(
            "<< /Type /Annot /Subtype /Widget /FT /Sig /Rect [0 0 0 0] /F 132 /T (Signature1) /V {sig_n} 0 R /P {page} 0 R >>"
        ),
    );
    let xref_pos = pdf.len();
    let mut x = String::from("xref\n");
    let mut sorted = offsets.clone();
    sorted.sort();
    for (n, off) in &sorted {
        x.push_str(&format!("{n} 1\n{off:010} 00000 n \n"));
    }
    x.push_str(&format!(
        "trailer\n<< /Size {} /Root {root} 0 R /Prev {prev} >>\nstartxref\n{xref_pos}\n%%EOF\n",
        size + 2
    ));
    pdf.extend_from_slice(x.as_bytes());

    // Locate the /Contents hex string and the /ByteRange placeholder inside the signature object.
    let tail = &pdf[sig_start..];
    let br = sig_start + find_last(tail, b"/ByteRange [").ok_or("internal: no ByteRange")?;
    let c = sig_start + find_last(tail, b"/Contents <").ok_or("internal: no Contents")? + 10;
    let (c_start, c_end) = (c, c + SIG_BYTES * 2 + 2); // `<`..`>` inclusive
    let (a2, l2) = (c_end, pdf.len() - c_end);
    let range = format!("[0 {c_start:010} {a2:010} {l2:010}]");
    pdf[br + 11..br + 11 + range.len()].copy_from_slice(range.as_bytes());

    let mut signed = pdf[..c_start].to_vec();
    signed.extend_from_slice(&pdf[c_end..]);
    let sig = cms_signature(key_pem, cert_pem, &signed)?;
    if sig.len() > SIG_BYTES {
        return Err("signature does not fit the reserved space".into());
    }
    let hex: String = sig.iter().map(|b| format!("{b:02x}")).collect();
    pdf[c_start + 1..c_start + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(pdf)
}

/// One verified signature: who signed and whether the digest and signature both check out.
pub fn verify_pdf(pdf: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use signature::Verifier;
    let text = String::from_utf8_lossy(pdf);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("/ByteRange [") {
        let at = from + i + 12;
        from = at;
        let Some(end) = text[at..].find(']') else {
            break;
        };
        let nums: Vec<usize> = text[at..at + end]
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        if nums.len() != 4 || nums[2] < nums[1] + 2 || nums[2] + nums[3] > pdf.len() {
            out.push(serde_json::json!({ "ok": false, "error": "bad ByteRange" }));
            continue;
        }
        let mut signed = pdf[nums[0]..nums[0] + nums[1]].to_vec();
        signed.extend_from_slice(&pdf[nums[2]..nums[2] + nums[3]]);
        let hex = String::from_utf8_lossy(&pdf[nums[1] + 1..nums[2] - 1]).into_owned();
        let hex = hex.trim_end_matches('0');
        let hex = if hex.len() % 2 == 1 {
            format!("{hex}0")
        } else {
            hex.to_string()
        };
        let der: Result<Vec<u8>, _> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect();
        let r = (|| -> Result<(String, bool, bool), String> {
            let der = der.map_err(|_| "bad signature hex".to_string())?;
            let ci = ContentInfo::from_der(&der).map_err(|e| e.to_string())?;
            let sd: cms::signed_data::SignedData =
                ci.content.decode_as().map_err(|e| e.to_string())?;
            let si = sd.signer_infos.0.iter().next().ok_or("no signer")?;
            let cert = sd
                .certificates
                .as_ref()
                .and_then(|c| {
                    c.0.iter().find_map(|c| match c {
                        CertificateChoices::Certificate(c) => Some(c.clone()),
                        _ => None,
                    })
                })
                .ok_or("no certificate in the signature")?;
            let attrs = si.signed_attrs.as_ref().ok_or("no signed attributes")?;
            let md = attrs
                .iter()
                .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)
                .and_then(|a| a.values.iter().next())
                .and_then(|v| OctetString::from_der(&v.to_der().ok()?).ok())
                .ok_or("no message digest")?;
            let digest_ok = md.as_bytes() == Sha256::digest(&signed).as_slice();
            let spki = cert
                .tbs_certificate
                .subject_public_key_info
                .to_der()
                .map_err(|e| e.to_string())?;
            let pk = rsa::RsaPublicKey::from_public_key_der(&spki)
                .map_err(|_| "only RSA signatures are supported".to_string())?;
            let vk = VerifyingKey::<Sha256>::new(pk);
            let sig = Signature::try_from(si.signature.as_bytes()).map_err(|e| e.to_string())?;
            let sig_ok = vk
                .verify(&attrs.to_der().map_err(|e| e.to_string())?, &sig)
                .is_ok();
            Ok((cert.tbs_certificate.subject.to_string(), digest_ok, sig_ok))
        })();
        out.push(match r {
            Ok((who, d, s)) => serde_json::json!({
                "ok": d && s, "signer": who, "digest_ok": d, "signature_ok": s,
                "covers_whole_file": nums[2] + nums[3] == pdf.len(),
                "note": "the certificate is not checked against a trust store"
            }),
            Err(e) => serde_json::json!({ "ok": false, "error": e }),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_editing_helpers() {
        assert_eq!(
            add_to_dict("<< /A 1 >>", "/B 2").unwrap(),
            "<< /A 1  /B 2 >>"
        );
        assert_eq!(number_after("/Size 12 /Root 3 0 R", "/Root"), Some(3));
        assert_eq!(number_after("page 1: 4 0 R", "page 1:"), Some(4));
    }

    #[test]
    fn signs_and_verifies_with_openssl() {
        let qpdf = "C:/Program Files/qpdf 12.4.2/bin/qpdf.exe";
        let have = |c: &str, a: &str| Command::new(c).arg(a).output().is_ok();
        if !have("openssl", "version") || !have(qpdf, "--version") {
            eprintln!("openssl or qpdf missing; skipped");
            return;
        }
        let d = std::env::temp_dir().join(format!("kb-sign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (key, crt) = (d.join("k.pem"), d.join("c.pem"));
        let ok = Command::new("openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=Test", "-days", "2",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&crt)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok);
        // A one-page PDF made by qpdf.
        let src = d.join("in.pdf");
        std::fs::write(
            &src,
            "%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>
endobj
trailer
<< /Root 1 0 R /Size 4 >>
%%EOF
",
        )
        .unwrap();
        let out = d.join("out.pdf");
        sign_pdf(
            qpdf,
            &src,
            &out,
            &std::fs::read_to_string(&key).unwrap(),
            &std::fs::read_to_string(&crt).unwrap(),
            "Test",
        )
        .unwrap();
        let check = Command::new(qpdf)
            .arg("--check")
            .arg(&out)
            .output()
            .unwrap();
        assert!(
            check.status.code() != Some(2),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );
        // Re-extract the signed ranges and the signature, then verify with OpenSSL.
        let pdf = std::fs::read(&out).unwrap();
        let text = String::from_utf8_lossy(&pdf).into_owned();
        let br = text.find("/ByteRange [").unwrap() + 12;
        let nums: Vec<usize> = text[br..]
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let mut signed = pdf[nums[0]..nums[0] + nums[1]].to_vec();
        signed.extend_from_slice(&pdf[nums[2]..nums[2] + nums[3]]);
        let hex = String::from_utf8_lossy(&pdf[nums[1] + 1..nums[2] - 1]).into_owned();
        let hex = hex.trim_end_matches('0');
        let hex = if hex.len() % 2 == 1 {
            format!("{hex}0")
        } else {
            hex.to_string()
        };
        let der: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        std::fs::write(d.join("s.der"), der).unwrap();
        std::fs::write(d.join("signed.bin"), signed).unwrap();
        let v = Command::new("openssl")
            .args([
                "cms",
                "-verify",
                "-binary",
                "-inform",
                "DER",
                "-noverify",
                "-in",
            ])
            .arg(d.join("s.der"))
            .arg("-content")
            .arg(d.join("signed.bin"))
            .arg("-out")
            .arg(d.join("o.bin"))
            .output()
            .unwrap();
        assert!(v.status.success(), "{}", String::from_utf8_lossy(&v.stderr));
        let v = verify_pdf(&pdf).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["ok"], true, "{:?}", v[0]);
        let mut bad = pdf.clone();
        bad[20] ^= 1;
        assert_eq!(verify_pdf(&bad).unwrap()[0]["ok"], false);
        let _ = std::fs::remove_dir_all(&d);
    }
}
