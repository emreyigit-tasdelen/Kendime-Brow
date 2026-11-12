//! Launch the test host (`kb-echo-host`) in the sandbox and talk to it over real IPC.

use std::path::PathBuf;

use kb_ipc::launch::{RestartPolicy, launch, should_restart, wait_exit};
use kb_ipc::{pb, recv, send};
use kb_platform::sandbox::SpawnSpec;

fn spec() -> SpawnSpec {
    SpawnSpec {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kb-echo-host")),
        memory_limit: Some(256 * 1024 * 1024),
        max_processes: 1,
        ..Default::default()
    }
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
async fn sandboxed_host_round_trip() {
    let mut host = launch("echo", spec()).await.unwrap();
    assert_eq!(host.module, "kb-echo-host");
    assert_eq!(call(&mut host, "echo", b"hi").await.body, b"hi");
}

/// Windows: the host runs at LOW integrity, so writing into a normal (medium) directory fails.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sandboxed_host_cannot_write_user_files() {
    let dir = std::env::temp_dir().join(format!("kb-sandbox-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("should-not-exist.txt");
    let mut host = launch("echo", spec()).await.unwrap();
    let reply = call(&mut host, "try-write", target.to_string_lossy().as_bytes()).await;
    assert!(
        !reply.error.is_empty(),
        "low-integrity host wrote a user file"
    );
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_is_detected_and_restarted() {
    let policy = RestartPolicy::default();
    let mut host = launch("echo", spec()).await.unwrap();
    send(
        &mut *host.stream,
        &pb::Envelope {
            id: 1,
            method: "crash".into(),
            body: vec![],
            error: String::new(),
        },
    )
    .await
    .unwrap();
    let code = wait_exit(host.child.clone()).await.unwrap();
    assert!(
        should_restart(code, 0, &policy),
        "crash exit code {code:#x}"
    );

    // Restart: a new host with a new endpoint and token works again.
    let mut again = launch("echo", spec()).await.unwrap();
    assert_eq!(call(&mut again, "echo", b"back").await.body, b"back");

    // Clean exit is not restarted.
    send(
        &mut *again.stream,
        &pb::Envelope {
            id: 2,
            method: "exit".into(),
            body: vec![0],
            error: String::new(),
        },
    )
    .await
    .unwrap();
    assert!(!should_restart(
        wait_exit(again.child.clone()).await.unwrap(),
        0,
        &policy
    ));
}
