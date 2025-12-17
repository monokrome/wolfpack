use libp2p::request_response;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Protocol name for wolfpack sync
pub const PROTOCOL_NAME: libp2p::StreamProtocol =
    libp2p::StreamProtocol::new("/wolfpack/sync/1.0.0");

/// Request types for the sync protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncRequest {
    /// Request peer's vector clock to compare state
    GetClock,

    /// Request events newer than the given clock
    GetEvents {
        /// Our current vector clock
        clock: HashMap<String, u64>,
    },

    /// Send events to peer
    PushEvents {
        /// Encrypted event data
        events: Vec<EncryptedEvent>,
    },

    /// Send a tab to this device
    SendTab {
        /// URL to open
        url: String,
        /// Optional title
        title: Option<String>,
        /// Sender device name
        from_device: String,
    },
}

/// Response types for the sync protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResponse {
    /// Return our vector clock
    Clock {
        clock: HashMap<String, u64>,
        device_id: String,
        device_name: String,
    },

    /// Return events the requester is missing
    Events { events: Vec<EncryptedEvent> },

    /// Acknowledge received events
    Ack { count: usize },

    /// Acknowledge received tab
    TabReceived,

    /// Error response
    Error { message: String },
}

/// Encrypted event for transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEvent {
    /// Unique event ID
    pub id: String,
    /// Originating device
    pub device_id: String,
    /// Vector clock counter for this device
    pub counter: u64,
    /// Encrypted payload (already E2E encrypted)
    pub ciphertext: Vec<u8>,
    /// Sender's public key (for decryption)
    pub public_key: Vec<u8>,
    /// Cipher used (1 = AES-GCM, 2 = XChaCha20)
    pub cipher: u8,
    /// Nonce used for encryption
    pub nonce: Vec<u8>,
}

/// Codec for serializing/deserializing sync messages
#[derive(Debug, Clone, Default)]
pub struct SyncCodec;

impl request_response::Codec for SyncCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = SyncRequest;
    type Response = SyncResponse;

    fn read_request<'life0, 'life1, 'life2, 'async_trait, T>(
        &'life0 mut self,
        _protocol: &'life1 Self::Protocol,
        io: &'life2 mut T,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<Self::Request>> + Send + 'async_trait>,
    >
    where
        T: futures::AsyncRead + Unpin + Send + 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let mut buf = Vec::new();
            futures::AsyncReadExt::read_to_end(io, &mut buf).await?;
            serde_json::from_slice(&buf)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })
    }

    fn read_response<'life0, 'life1, 'life2, 'async_trait, T>(
        &'life0 mut self,
        _protocol: &'life1 Self::Protocol,
        io: &'life2 mut T,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::io::Result<Self::Response>> + Send + 'async_trait,
        >,
    >
    where
        T: futures::AsyncRead + Unpin + Send + 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let mut buf = Vec::new();
            futures::AsyncReadExt::read_to_end(io, &mut buf).await?;
            serde_json::from_slice(&buf)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })
    }

    fn write_request<'life0, 'life1, 'life2, 'async_trait, T>(
        &'life0 mut self,
        _protocol: &'life1 Self::Protocol,
        io: &'life2 mut T,
        req: Self::Request,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'async_trait>,
    >
    where
        T: futures::AsyncWrite + Unpin + Send + 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let buf = serde_json::to_vec(&req)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            futures::AsyncWriteExt::write_all(io, &buf).await?;
            futures::AsyncWriteExt::close(io).await?;
            Ok(())
        })
    }

    fn write_response<'life0, 'life1, 'life2, 'async_trait, T>(
        &'life0 mut self,
        _protocol: &'life1 Self::Protocol,
        io: &'life2 mut T,
        res: Self::Response,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'async_trait>,
    >
    where
        T: futures::AsyncWrite + Unpin + Send + 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let buf = serde_json::to_vec(&res)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            futures::AsyncWriteExt::write_all(io, &buf).await?;
            futures::AsyncWriteExt::close(io).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_name() {
        assert_eq!(PROTOCOL_NAME.as_ref(), "/wolfpack/sync/1.0.0");
    }

    #[test]
    fn test_sync_request_get_clock_serialize() {
        let req = SyncRequest::GetClock;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: SyncRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SyncRequest::GetClock));
    }

    #[test]
    fn test_sync_request_get_events_serialize() {
        let mut clock = HashMap::new();
        clock.insert("device-a".to_string(), 5);
        clock.insert("device-b".to_string(), 3);

        let req = SyncRequest::GetEvents { clock };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: SyncRequest = serde_json::from_str(&json).unwrap();

        if let SyncRequest::GetEvents { clock } = parsed {
            assert_eq!(clock.get("device-a"), Some(&5));
            assert_eq!(clock.get("device-b"), Some(&3));
        } else {
            panic!("Expected GetEvents");
        }
    }

    #[test]
    fn test_sync_request_push_events_serialize() {
        let events = vec![EncryptedEvent {
            id: "event-1".to_string(),
            device_id: "device-a".to_string(),
            counter: 1,
            ciphertext: vec![1, 2, 3],
            public_key: vec![4, 5, 6],
            cipher: 1,
            nonce: vec![7, 8, 9],
        }];

        let req = SyncRequest::PushEvents { events };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: SyncRequest = serde_json::from_str(&json).unwrap();

        if let SyncRequest::PushEvents { events } = parsed {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id, "event-1");
            assert_eq!(events[0].device_id, "device-a");
            assert_eq!(events[0].counter, 1);
            assert_eq!(events[0].cipher, 1);
        } else {
            panic!("Expected PushEvents");
        }
    }

    #[test]
    fn test_sync_request_send_tab_serialize() {
        let req = SyncRequest::SendTab {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            from_device: "device-a".to_string(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: SyncRequest = serde_json::from_str(&json).unwrap();

        if let SyncRequest::SendTab {
            url,
            title,
            from_device,
        } = parsed
        {
            assert_eq!(url, "https://example.com");
            assert_eq!(title, Some("Example".to_string()));
            assert_eq!(from_device, "device-a");
        } else {
            panic!("Expected SendTab");
        }
    }

    #[test]
    fn test_sync_request_send_tab_no_title() {
        let req = SyncRequest::SendTab {
            url: "https://example.com".to_string(),
            title: None,
            from_device: "device-a".to_string(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: SyncRequest = serde_json::from_str(&json).unwrap();

        if let SyncRequest::SendTab { title, .. } = parsed {
            assert!(title.is_none());
        } else {
            panic!("Expected SendTab");
        }
    }

    #[test]
    fn test_sync_response_clock_serialize() {
        let mut clock = HashMap::new();
        clock.insert("device-a".to_string(), 10);

        let res = SyncResponse::Clock {
            clock,
            device_id: "device-a".to_string(),
            device_name: "My Device".to_string(),
        };

        let json = serde_json::to_string(&res).unwrap();
        let parsed: SyncResponse = serde_json::from_str(&json).unwrap();

        if let SyncResponse::Clock {
            clock,
            device_id,
            device_name,
        } = parsed
        {
            assert_eq!(clock.get("device-a"), Some(&10));
            assert_eq!(device_id, "device-a");
            assert_eq!(device_name, "My Device");
        } else {
            panic!("Expected Clock");
        }
    }

    #[test]
    fn test_sync_response_events_serialize() {
        let events = vec![EncryptedEvent {
            id: "event-1".to_string(),
            device_id: "device-a".to_string(),
            counter: 1,
            ciphertext: vec![1, 2, 3],
            public_key: vec![4, 5, 6],
            cipher: 2, // XChaCha20
            nonce: vec![7, 8, 9],
        }];

        let res = SyncResponse::Events { events };
        let json = serde_json::to_string(&res).unwrap();
        let parsed: SyncResponse = serde_json::from_str(&json).unwrap();

        if let SyncResponse::Events { events } = parsed {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].cipher, 2);
        } else {
            panic!("Expected Events");
        }
    }

    #[test]
    fn test_sync_response_ack_serialize() {
        let res = SyncResponse::Ack { count: 5 };
        let json = serde_json::to_string(&res).unwrap();
        let parsed: SyncResponse = serde_json::from_str(&json).unwrap();

        if let SyncResponse::Ack { count } = parsed {
            assert_eq!(count, 5);
        } else {
            panic!("Expected Ack");
        }
    }

    #[test]
    fn test_sync_response_tab_received_serialize() {
        let res = SyncResponse::TabReceived;
        let json = serde_json::to_string(&res).unwrap();
        let parsed: SyncResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SyncResponse::TabReceived));
    }

    #[test]
    fn test_sync_response_error_serialize() {
        let res = SyncResponse::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&res).unwrap();
        let parsed: SyncResponse = serde_json::from_str(&json).unwrap();

        if let SyncResponse::Error { message } = parsed {
            assert_eq!(message, "Something went wrong");
        } else {
            panic!("Expected Error");
        }
    }

    #[test]
    fn test_encrypted_event_serialize() {
        let event = EncryptedEvent {
            id: "uuid-here".to_string(),
            device_id: "device-123".to_string(),
            counter: 42,
            ciphertext: vec![0xde, 0xad, 0xbe, 0xef],
            public_key: vec![0x01, 0x02, 0x03],
            cipher: 1,
            nonce: vec![0x0a, 0x0b, 0x0c],
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: EncryptedEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "uuid-here");
        assert_eq!(parsed.device_id, "device-123");
        assert_eq!(parsed.counter, 42);
        assert_eq!(parsed.ciphertext, vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(parsed.public_key, vec![0x01, 0x02, 0x03]);
        assert_eq!(parsed.cipher, 1);
        assert_eq!(parsed.nonce, vec![0x0a, 0x0b, 0x0c]);
    }

    #[test]
    fn test_sync_codec_default() {
        let codec = SyncCodec;
        // Just verify it can be created
        let _ = codec;
    }
}
