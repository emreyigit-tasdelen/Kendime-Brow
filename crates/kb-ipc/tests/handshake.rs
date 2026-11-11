//! End-to-end over a real OS endpoint (named pipe on Windows, Unix socket elsewhere).

use kb_ipc::{
    Token, accept_handshake, client_handshake, connect, endpoint_name, listen, pb, recv, send,
};

#[tokio::test(flavor = "current_thread")]
async fn handshake_and_envelope_round_trip() {
    let name = endpoint_name("test").unwrap();
    let token = Token::generate().unwrap();
    let listener = listen(&name).unwrap();

    let server_token = token.clone();
    let server = async move {
        let mut s = listener.accept().await.unwrap();
        let module = accept_handshake(&mut *s, &server_token).await.unwrap();
        let req: pb::Envelope = recv(&mut *s).await.unwrap();
        send(
            &mut *s,
            &pb::Envelope {
                id: req.id,
                method: req.method.clone(),
                body: req.body.iter().rev().copied().collect(),
                error: String::new(),
            },
        )
        .await
        .unwrap();
        module
    };
    let client = async {
        let mut c = connect(&name).await.unwrap();
        client_handshake(&mut *c, &token, "kb-test").await.unwrap();
        send(
            &mut *c,
            &pb::Envelope {
                id: 7,
                method: "echo.reverse".into(),
                body: b"abc".to_vec(),
                error: String::new(),
            },
        )
        .await
        .unwrap();
        recv::<_, pb::Envelope>(&mut *c).await.unwrap()
    };
    let (module, reply) = tokio::join!(server, client);
    assert_eq!(module, "kb-test");
    assert_eq!((reply.id, reply.body.as_slice()), (7, b"cba".as_slice()));
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_token_is_rejected() {
    let name = endpoint_name("test").unwrap();
    let token = Token::generate().unwrap();
    let wrong = Token::generate().unwrap();
    let listener = listen(&name).unwrap();

    let server = async move {
        let mut s = listener.accept().await.unwrap();
        accept_handshake(&mut *s, &token).await
    };
    let client = async {
        let mut c = connect(&name).await.unwrap();
        client_handshake(&mut *c, &wrong, "evil").await
    };
    let (s, c) = tokio::join!(server, client);
    assert_eq!(s.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(c.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
}

#[tokio::test(flavor = "current_thread")]
async fn second_listener_on_same_name_fails() {
    let name = endpoint_name("test").unwrap();
    let _first = listen(&name).unwrap();
    assert!(listen(&name).is_err());
}
