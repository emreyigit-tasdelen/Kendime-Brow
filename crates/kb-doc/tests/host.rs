//! Launches `kb-doc-host` in the sandbox (restricted token, low integrity, Job Object) and renders
//! a PDF through it. Skipped when PDFium is not available (`PDFIUM_PATH`).

use std::path::PathBuf;

use kb_ipc::launch::launch;
use kb_ipc::{pb, recv, send};
use kb_platform::sandbox::SpawnSpec;

fn sample_pdf() -> Vec<u8> {
    let stream = "BT /F1 24 Tf 20 40 Td (Sandboxed) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

async fn call(host: &mut kb_ipc::launch::Host, method: &str, body: &[u8]) -> pb::Envelope {
    send(
        &mut *host.stream,
        &pb::Envelope {
            id: 1,
            method: method.into(),
            body: body.to_vec(),
            error: String::new(),
        },
    )
    .await
    .unwrap();
    recv(&mut *host.stream).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renders_through_the_sandboxed_host() {
    if kb_doc::load_pdfium().is_err() {
        eprintln!("PDFium library not found (set PDFIUM_PATH) — skipping");
        return;
    }
    let spec = SpawnSpec {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kb-doc-host")),
        memory_limit: Some(1024 * 1024 * 1024),
        max_processes: 1,
        ..Default::default()
    };
    let mut host = launch("doc", spec)
        .await
        .expect("host did not start/connect");
    assert_eq!(host.module, "kb-doc");

    let open = call(&mut host, "pdf.open", &sample_pdf()).await;
    assert!(open.error.is_empty(), "{}", open.error);
    let reply: kb_doc::service::OpenReply = serde_json::from_slice(&open.body).unwrap();
    assert_eq!((reply.pages, reply.width.round() as i32), (1, 200));

    let png = call(&mut host, "pdf.render", br#"{"page":0,"dpi":144}"#).await;
    assert!(png.error.is_empty(), "{}", png.error);
    assert!(png.body.starts_with(&[0x89, b'P', b'N', b'G']));

    assert!(
        !call(&mut host, "pdf.render", br#"{"page":9}"#)
            .await
            .error
            .is_empty()
    );
    assert!(!call(&mut host, "pdf.nope", b"").await.error.is_empty());
    call(&mut host, "pdf.close", b"").await;
    assert!(
        !call(&mut host, "pdf.render", br#"{"page":0}"#)
            .await
            .error
            .is_empty()
    );
}
