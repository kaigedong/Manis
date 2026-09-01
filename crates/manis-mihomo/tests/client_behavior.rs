use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use manis_core::{PolicyCandidateKind, PolicyGroupKind, RoutingMode};
#[cfg(unix)]
use manis_mihomo::UnixSocketTransport;
use manis_mihomo::{
    ConnectionsState, ControllerConfig, ControllerTransport, GroupKind, MihomoClient, MihomoError,
    RuntimeConfig, RuntimeTunConfig, StdHttpTransport, to_policy_catalog,
};
use serde_json::Value;

#[derive(Default)]
struct FakeTransport {
    requests: RefCell<Vec<RecordedRequest>>,
    initial_config: RefCell<Option<&'static str>>,
}

#[derive(Debug, Clone, PartialEq)]
struct RecordedRequest {
    method: &'static str,
    path: String,
    body: Option<Value>,
}

type TcpResponseServer = (String, std::thread::JoinHandle<std::io::Result<String>>);

#[cfg(unix)]
type UnixResponseServer = (
    std::path::PathBuf,
    std::thread::JoinHandle<std::io::Result<String>>,
);

impl RecordedRequest {
    fn get(path: &str) -> Self {
        Self {
            method: "GET",
            path: path.to_owned(),
            body: None,
        }
    }

    fn patch(path: &str, body: Value) -> Self {
        Self {
            method: "PATCH",
            path: path.to_owned(),
            body: Some(body),
        }
    }

    fn put(path: &str, body: Value) -> Self {
        Self {
            method: "PUT",
            path: path.to_owned(),
            body: Some(body),
        }
    }
}

impl ControllerTransport for FakeTransport {
    fn get(&self, _config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        self.requests.borrow_mut().push(RecordedRequest::get(path));
        match path {
            "/version" => Ok(r#"{"meta":true,"version":"v1.19.0","ignored":1}"#.to_owned()),
            "/proxies" => Ok(proxy_fixture()),
            "/providers/proxies" => Ok(provider_fixture()),
            "/group/Auto%20%F0%9F%8C%90%2FHK/delay?url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204%3Fx%3D1%26ok%3Dtrue&timeout=5000" => {
                Ok(r#"{"Japan 01":51,"US 01":238}"#.to_owned())
            }
            "/proxies/Japan%2001%20%5B%E5%80%8D%E7%8E%87%C3%971%5D/delay?url=http%3A%2F%2Fcp.cloudflare.com%2Fgenerate_204&timeout=1500" => {
                Ok(r#"{"delay":47}"#.to_owned())
            }
            "/providers/proxies/Subscription%201/HK%2001/healthcheck?url=http%3A%2F%2Fcp.cloudflare.com%2Fgenerate_204&timeout=1500" => {
                Ok(r#"{"delay":63}"#.to_owned())
            }
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select" => Ok(
                r#"{"name":"Proxy/🌐 Select","type":"Selector","now":"Japan 01","all":["Japan 01","US 01"],"unexpected":true}"#
                    .to_owned(),
            ),
            "/rules" => Ok(rule_fixture()),
            "/connections" => Ok(connection_fixture()),
            "/configs" => Ok(self
                .initial_config
                .borrow_mut()
                .take()
                .map_or_else(config_fixture, str::to_owned)),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected path {path}"
            ))),
        }
    }

    fn patch_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::patch(path, body.clone()));
        match path {
            "/configs" => Ok(String::new()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected patch path {path}"
            ))),
        }
    }

    fn put_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::put(path, body.clone()));
        match path {
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select" | "/configs?force=true" => Ok(String::new()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected put path {path}"
            ))),
        }
    }
}

#[derive(Default)]
struct TunRejectedTransport {
    requests: RefCell<Vec<RecordedRequest>>,
}

impl ControllerTransport for TunRejectedTransport {
    fn get(&self, _config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        self.requests.borrow_mut().push(RecordedRequest::get(path));
        Ok(r#"{"tun":{"enable":false}}"#.to_owned())
    }

    fn patch_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::patch(path, body.clone()));
        Ok(String::new())
    }

    fn put_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        _body: &Value,
    ) -> Result<String, MihomoError> {
        Err(MihomoError::InvalidResponse(format!(
            "unexpected put path {path}"
        )))
    }
}

#[path = "client_behavior/decoding.rs"]
mod decoding;
#[path = "client_behavior/mutations.rs"]
mod mutations;
#[path = "client_behavior/snapshot.rs"]
mod snapshot;
#[path = "client_behavior/transports.rs"]
mod transports;

fn proxy_fixture() -> String {
    r#"
    {
      "proxies": {
        "Proxy": {
          "name": "Proxy",
          "type": "Selector",
          "now": "Japan 01",
          "all": ["Japan 01", "US 01", "剩余流量：96.83 GB"],
          "history": [{"delay": 44}, {"delay": null}, {"delay": 38}],
          "providerName": "airport",
          "alive": true,
          "fixed": "Japan 01",
          "hidden": false
        },
        "Auto": {
          "name": "Auto",
          "type": "url-test",
          "now": "US 01",
          "all": ["Japan 01", "US 01"],
          "history": null
        },
        "GLOBAL": {
          "name": "GLOBAL",
          "type": "Selector",
          "now": "DIRECT",
          "all": ["DIRECT", "Proxy"]
        },
        "__MANIS_GLOBAL__": {
          "name": "__MANIS_GLOBAL__",
          "type": "Selector",
          "now": "Japan 01",
          "all": ["Japan 01", "US 01"]
        },
        "Japan 01": {
          "name": "Japan 01",
          "type": "ss",
          "history": [{"delay": 51}],
          "provider-name": "airport"
        },
        "US 01": {
          "name": "US 01",
          "type": "ss",
          "history": [{"delay": 0}]
        }
      }
    }
    "#
    .to_owned()
}

fn provider_fixture() -> String {
    r#"
    {
      "providers": {
        "airport": {
          "name": "airport",
          "type": "Proxy",
          "vehicleType": "HTTP",
          "updatedAt": "2026-08-25T00:00:00Z",
          "proxies": [
            {
              "name": "Japan 01",
              "type": "VLESS",
              "alive": true,
              "history": [{"delay": 51}]
            },
            {
              "name": "US 01",
              "type": "Trojan",
              "alive": false,
              "history": []
            },
            {
              "name": "剩余流量：96.83 GB",
              "type": "Trojan",
              "alive": false,
              "history": [{"delay": 65535}]
            }
          ]
        }
      }
    }
    "#
    .to_owned()
}

fn rule_fixture() -> String {
    r#"
    {
      "rules": [
        {
          "index": 0,
          "type": "DOMAIN-SUFFIX",
          "payload": "example.com",
          "proxy": "Proxy",
          "size": -1,
          "extra": {"hitCount": 12, "missCount": 3, "disabled": false, "ignored": true}
        },
        {
          "index": 1,
          "type": "MATCH",
          "payload": "",
          "proxy": "DIRECT",
          "size": -1,
          "extra": {"disabled": true}
        }
      ]
    }
    "#
    .to_owned()
}

fn connection_fixture() -> String {
    r#"
    {
      "downloadTotal": 2048,
      "uploadTotal": 1024,
      "connections": [
        {
          "id": "abc",
          "metadata": {
            "host": "",
            "destinationIP": "93.184.216.34",
            "process": "curl",
            "destinationPort": 443,
            "network": "tcp",
            "type": "HTTP"
          },
          "chains": ["Japan 01", "Proxy"],
          "providerChains": [["airport", "Japan 01"]],
          "rule": "DOMAIN-SUFFIX",
          "rulePayload": "example.com",
          "upload": 100,
          "download": 200,
          "start": "2026-08-25T00:00:00Z"
        }
      ]
    }
    "#
    .to_owned()
}

fn config_fixture() -> String {
    r#"
    {
      "port": 7891,
      "socks-port": 7892,
      "mixed-port": 7890,
      "mode": "rule",
      "tun": {
        "enable": true,
        "stack": "system",
        "auto-route": true,
        "auto-detect-interface": true,
        "dns-hijack": ["0.0.0.0:53"]
      }
    }
    "#
    .to_owned()
}

fn spawn_delayed_response_server(
    delay: Duration,
    response: &str,
) -> Result<(String, std::thread::JoinHandle<String>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let response = response.to_owned();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return String::new();
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_request(&mut stream).unwrap();
        std::thread::sleep(delay);
        let _ = stream.write_all(response.as_bytes());
        request
    });

    Ok((address, handle))
}

fn spawn_trickling_incomplete_header_server()
-> Result<(String, std::thread::JoinHandle<String>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return String::new();
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_request(&mut stream).unwrap();
        std::thread::sleep(Duration::from_millis(70));
        for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n" {
            let _ = stream.write_all(&[*byte]);
            std::thread::sleep(Duration::from_millis(30));
        }
        std::thread::sleep(Duration::from_millis(500));
        request
    });

    Ok((address, handle))
}

fn spawn_fragmented_response_header_server() -> Result<TcpResponseServer, Box<dyn std::error::Error>>
{
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let handle = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _peer) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream.set_nodelay(true)?;
        let request = read_request(&mut stream)?;
        // A few deliberate fragments avoid accumulating a scheduler delay for
        // every byte. Each gap still exceeds the 40 ms send-phase timeout, and
        // both a header name and the terminating CRLF are split across reads.
        stream.write_all(b"HTTP/1.1 200 OK\r\nCont")?;
        for fragment in [
            &b"ent-Length: 13\r"[..],
            &b"\n\r"[..],
            &b"\n{\"meta\":true}"[..],
        ] {
            std::thread::sleep(Duration::from_millis(100));
            stream.write_all(fragment)?;
        }
        Ok(request)
    });

    Ok((address, handle))
}

fn spawn_delayed_body_server(
    delay: Duration,
) -> Result<(String, std::thread::JoinHandle<String>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return String::new();
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_request(&mut stream).unwrap();
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n");
        std::thread::sleep(delay);
        let _ = stream.write_all(b"{\"meta\":true}");
        request
    });

    Ok((address, handle))
}

#[cfg(unix)]
fn spawn_delayed_unix_response_server(
    label: &str,
    delay: Duration,
    response: &str,
) -> Result<UnixResponseServer, Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path =
        std::env::temp_dir().join(format!("mm-{label}-{}-{unique}.sock", std::process::id()));
    let listener = UnixListener::bind(&socket_path)?;
    let response = response.to_owned();
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let request = read_request(&mut stream)?;
        std::thread::sleep(delay);
        stream.write_all(response.as_bytes())?;
        Ok(request)
    });

    Ok((socket_path, server))
}

fn spawn_one_response_server(
    response: &str,
) -> Result<(String, std::thread::JoinHandle<String>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let response = response.to_owned();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return String::new();
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_request(&mut stream).unwrap();
        let _ = stream.write_all(response.as_bytes());
        request
    });

    Ok((address, handle))
}

fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request
        .split("\r\n\r\n")
        .next()?
        .lines()
        .skip(1)
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
}

fn read_request(stream: &mut impl Read) -> std::io::Result<String> {
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte)?;
        request.push(byte[0]);
        assert!(
            request.len() <= 64 * 1024,
            "fixture request headers too large"
        );
    }
    let headers = String::from_utf8(request.clone()).unwrap();
    let length = header_value(&headers, "content-length")
        .unwrap_or("0")
        .parse::<usize>()
        .unwrap();
    assert!(length <= 1024 * 1024);
    let start = request.len();
    request.resize(start + length, 0);
    stream.read_exact(&mut request[start..])?;
    Ok(String::from_utf8(request).unwrap())
}
