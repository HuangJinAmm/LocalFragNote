//! protocol.rs 单元测试：帧编解码 + JSON 类型往返

use memos_app::lan::protocol::*;

#[test]
fn test_frame_roundtrip_basic() {
    let payload = br#"{"method":"GetProfile","params":null}"#;
    let mut buf = Vec::new();
    write_frame(&mut buf, payload).unwrap();
    let mut reader = &buf[..];
    let decoded = read_frame(&mut reader).unwrap();
    assert_eq!(decoded, payload);
}

#[test]
fn test_frame_roundtrip_empty_payload() {
    let payload = b"";
    let mut buf = Vec::new();
    write_frame(&mut buf, payload).unwrap();
    let mut reader = &buf[..];
    let decoded = read_frame(&mut reader).unwrap();
    assert_eq!(decoded, payload);
}

#[test]
fn test_frame_max_size_rejected() {
    let oversized_len: u32 = (MAX_FRAME_SIZE as u32) + 1;
    let mut buf = Vec::new();
    buf.extend_from_slice(&oversized_len.to_be_bytes());
    buf.push(0);
    let mut reader = &buf[..];
    let result = read_frame(&mut reader);
    assert!(result.is_err(), "超过 16MB 的帧应被拒绝");
}

#[test]
fn test_request_getprofile_serialization() {
    let req = Request::GetProfile;
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"method\":\"GetProfile\""));
    let decoded: Request = serde_json::from_str(&json).unwrap();
    match decoded {
        Request::GetProfile => (),
        _ => panic!("应为 GetProfile 变体"),
    }
}

#[test]
fn test_request_listmemos_serialization() {
    let req = Request::ListMemos {
        offset: 0,
        limit: 50,
        tag_filter: Some(vec!["work".to_string()]),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"method\":\"ListMemos\""));
    let decoded: Request = serde_json::from_str(&json).unwrap();
    match decoded {
        Request::ListMemos { offset, limit, tag_filter } => {
            assert_eq!(offset, 0);
            assert_eq!(limit, 50);
            assert_eq!(tag_filter, Some(vec!["work".to_string()]));
        }
        _ => panic!("应为 ListMemos 变体"),
    }
}

#[test]
fn test_response_ok_serialization() {
    let resp = Response::Ok {
        data: ResponseData::Profile {
            display_name: "Alice".to_string(),
            public_memo_count: 42,
            tags: vec!["work".to_string()],
        },
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"status\":\"Ok\""));
    let decoded: Response = serde_json::from_str(&json).unwrap();
    match decoded {
        Response::Ok { data } => match data {
            ResponseData::Profile { display_name, public_memo_count, tags } => {
                assert_eq!(display_name, "Alice");
                assert_eq!(public_memo_count, 42);
                assert_eq!(tags, vec!["work"]);
            }
            _ => panic!("应为 Profile 变体"),
        },
        _ => panic!("应为 Ok 变体"),
    }
}

#[test]
fn test_response_err_serialization() {
    let resp = Response::Err {
        code: 403,
        message: "forbidden".to_string(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"status\":\"Err\""));
    assert!(json.contains("403"));
}

#[test]
fn test_unknown_method_rejected() {
    let json = r#"{"method":"Nonexistent","params":null}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_err(), "未知 method 应反序列化失败");
}
