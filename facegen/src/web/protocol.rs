//! Wire format between the harness page and the server.
//!
//! Text messages are JSON tagged by `type`. Binary messages are frames:
//! a little-endian u32 sequence number, a u32 layout generation, then
//! the BGRA atlas bytes. The page drops any frame whose generation does
//! not match the layout it last received.

use serde::{Deserialize, Serialize};

use crate::app::LayoutInfo;
use crate::layout::Layout;
use crate::sinks::Frame;

pub const FRAME_HEADER_BYTES: usize = 8;

/// Server to page.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage<'a> {
    Layout(&'a LayoutInfo),
    Error { message: String },
}

/// Page to server.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    /// Switch to a preset by name, or apply a full layout.
    Layout {
        #[serde(default)]
        preset: Option<String>,
        #[serde(default)]
        layout: Option<Layout>,
    },
    /// Set one named input, as a slider would.
    Input { name: String, value: f32 },
}

pub fn encode_frame(frame: &Frame, generation: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + frame.bgra.len());
    out.extend_from_slice(&(frame.seq as u32).to_le_bytes());
    out.extend_from_slice(&generation.to_le_bytes());
    out.extend_from_slice(&frame.bgra);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_message_is_the_header_followed_by_the_pixels() {
        let frame = Frame {
            seq: 0x0102_0304,
            width: 1,
            height: 1,
            bgra: vec![10, 20, 30, 255],
        };
        let bytes = encode_frame(&frame, 7);
        assert_eq!(&bytes[..4], &[4, 3, 2, 1], "sequence is little-endian u32");
        assert_eq!(
            &bytes[4..8],
            &[7, 0, 0, 0],
            "generation is little-endian u32"
        );
        assert_eq!(
            &bytes[8..],
            &[10, 20, 30, 255],
            "pixels follow the header unchanged"
        );
    }

    #[test]
    fn client_messages_parse_with_their_type_tag() {
        let m: ClientMessage =
            serde_json::from_str(r#"{"type":"layout","preset":"six_panel"}"#).unwrap();
        assert!(
            matches!(m, ClientMessage::Layout { preset: Some(ref p), layout: None } if p == "six_panel"),
            "preset message"
        );
        let m: ClientMessage =
            serde_json::from_str(r#"{"type":"input","name":"jawOpen","value":0.5}"#).unwrap();
        assert!(
            matches!(m, ClientMessage::Input { ref name, value } if name == "jawOpen" && value == 0.5),
            "input message"
        );
        assert!(
            serde_json::from_str::<ClientMessage>(r#"{"type":"dance"}"#).is_err(),
            "unknown type must fail"
        );
    }

    #[test]
    fn server_error_messages_carry_the_type_tag() {
        let text = serde_json::to_string(&ServerMessage::Error {
            message: "bad".into(),
        })
        .unwrap();
        assert_eq!(
            text, r#"{"type":"error","message":"bad"}"#,
            "error message shape"
        );
    }
}
