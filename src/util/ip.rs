use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// 通过 UDP socket 探测本机局域网 IP
pub fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;

    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ipv4) if !ipv4.is_loopback() && ipv4 != Ipv4Addr::UNSPECIFIED => {
            Some(ipv4.to_string())
        }
        _ => None,
    }
}
