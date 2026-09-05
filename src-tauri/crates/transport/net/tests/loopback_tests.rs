//! 网络传输回环测试 — TCP server/client + UDP, 127.0.0.1 真实 socket

use std::time::Duration;

use tokio::time::timeout;
use vofa_core::{TcpClientConfig, TcpServerConfig, UdpConfig};

/// 探测一个空闲 TCP 端口 (bind :0 后立刻释放, 复用该端口号)
async fn free_tcp_port() -> u16 {
    tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定 :0")
        .local_addr()
        .expect("local_addr")
        .port()
}

#[tokio::test]
async fn tcp_server_and_client_exchange_frames() {
    let port = free_tcp_port().await;
    let server = net::tcp::spawn_server(TcpServerConfig {
        listen_addr: "127.0.0.1".into(),
        listen_port: port,
    })
    .await
    .expect("server bind");
    let client = net::tcp::spawn_client(TcpClientConfig {
        host: "127.0.0.1".into(),
        port,
    })
    .await
    .expect("client connect");

    // client → server
    client
        .0
        .send(b"ping".to_vec())
        .await
        .expect("client 写通道");
    let mut server_rx = server.1.subscribe();
    let got = timeout(Duration::from_secs(2), server_rx.recv())
        .await
        .expect("server 应在超时内收到")
        .expect("广播");
    assert_eq!(got, b"ping");

    // server → client
    server
        .0
        .send(b"pong".to_vec())
        .await
        .expect("server 写通道");
    let mut client_rx = client.1.subscribe();
    let got = timeout(Duration::from_secs(2), client_rx.recv())
        .await
        .expect("client 应在超时内收到")
        .expect("广播");
    assert_eq!(got, b"pong");
}

/// 连接拒绝 → TransportError::TcpConnect (host/port 可见, 供前端定位)
#[tokio::test]
async fn tcp_client_connect_refused_reports_error() {
    let port = free_tcp_port().await;
    let err = net::tcp::spawn_client(TcpClientConfig {
        host: "127.0.0.1".into(),
        port,
    })
    .await;
    let msg = match err {
        Err(e) => e.to_string(),
        Ok(_) => panic!("对无监听端口连接应失败"),
    };
    assert!(
        msg.contains("127.0.0.1") && msg.contains(&port.to_string()),
        "错误信息应含 host/port: {msg}"
    );
}

/// UDP 双端回环: A 发 → B 收
#[tokio::test]
async fn udp_peers_exchange_datagrams() {
    let port_a = free_tcp_port().await; // 空闲端口复用 (UDP/TCP 表独立)
    let port_b = free_tcp_port().await;

    let a = net::udp::spawn(UdpConfig {
        local_addr: "127.0.0.1".into(),
        remote_addr: "127.0.0.1".into(),
        local_port: port_a,
        remote_port: port_b,
    })
    .await
    .expect("udp A bind+connect");
    let b = net::udp::spawn(UdpConfig {
        local_addr: "127.0.0.1".into(),
        remote_addr: "127.0.0.1".into(),
        local_port: port_b,
        remote_port: port_a,
    })
    .await
    .expect("udp B bind+connect");

    // 等 A 的 connect 生效后再发送 (UDP 无握手, 首包可能因 ARP/路由短暂丢失)
    tokio::time::sleep(Duration::from_millis(50)).await;
    a.0.send(b"datagram".to_vec()).await.expect("udp 写通道");
    let mut b_rx = b.1.subscribe();
    let got = timeout(Duration::from_secs(2), b_rx.recv())
        .await
        .expect("B 应在超时内收到")
        .expect("广播");
    assert_eq!(got, b"datagram");
}
