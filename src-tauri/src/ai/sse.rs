//! SSE 流式响应解析：解析 OpenAI chat/completions 的 stream 格式
//!
//! SSE 协议：每行 `data: {json}\n\n`，最后 `data: [DONE]\n\n`
//! tool_calls 分多块到达，需按 index 拼接

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

/// 累积的 tool_call（OpenAI 流式协议中按 index 拼接）
#[derive(Debug, Clone, Default)]
pub struct ToolCallAccumulator {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 单条 SSE 事件解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// 文本内容增量（可能为空）
    pub content_delta: Option<String>,
    /// tool_calls 增量（按 index）
    pub tool_call_delta: Option<ToolCallDelta>,
    /// finish_reason（流结束时出现）
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: u32,
    pub id: Option<String>,
    pub function_name: Option<String>,
    pub arguments_chunk: Option<String>,
}

/// 解析一行 SSE data，返回 SseEvent
/// 输入行应为 `data: {...}` 或 `data: [DONE]`
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim();
    if !line.starts_with("data:") {
        return None;
    }
    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Some(SseEvent {
            content_delta: None,
            tool_call_delta: None,
            finish_reason: Some("[DONE]".to_string()),
        });
    }

    let json: Value = serde_json::from_str(data).ok()?;
    let choice = json.get("choices")?.get(0)?;
    let delta = choice.get("delta")?;
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content_delta = delta
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tool_call_delta = delta.get("tool_calls").and_then(|tcs| {
        let tc = tcs.get(0)?;
        let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let id = tc.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        let function_name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let arguments_chunk = tc
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if id.is_none() && function_name.is_none() && arguments_chunk.is_none() {
            None
        } else {
            Some(ToolCallDelta {
                index,
                id,
                function_name,
                arguments_chunk,
            })
        }
    });

    Some(SseEvent {
        content_delta,
        tool_call_delta,
        finish_reason,
    })
}

/// 从 reader 读取完整 SSE 流，累积 tool_calls，返回 (完整文本, tool_calls)
/// 每读到 content_delta 时调用 on_chunk 回调（用于流式推送）
pub fn read_sse_stream<R: Read, F: FnMut(&str)>(
    reader: R,
    mut on_chunk: F,
) -> std::io::Result<(String, Vec<ToolCallAccumulator>)> {
    let buf_reader = BufReader::new(reader);
    let mut full_content = String::new();
    let mut tool_calls: Vec<ToolCallAccumulator> = Vec::new();

    for line_result in buf_reader.lines() {
        let line = line_result?;
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(event) = parse_sse_line(&line) {
            if let Some(delta) = &event.content_delta {
                full_content.push_str(delta);
                on_chunk(delta);
            }
            if let Some(tc_delta) = &event.tool_call_delta {
                let idx = tc_delta.index as usize;
                while tool_calls.len() <= idx {
                    tool_calls.push(ToolCallAccumulator::default());
                    tool_calls[idx].index = idx as u32;
                }
                if let Some(id) = &tc_delta.id {
                    tool_calls[idx].id = id.clone();
                }
                if let Some(name) = &tc_delta.function_name {
                    tool_calls[idx].name = name.clone();
                }
                if let Some(args) = &tc_delta.arguments_chunk {
                    tool_calls[idx].arguments.push_str(args);
                }
            }
            if let Some(fr) = &event.finish_reason {
                if fr == "[DONE]" {
                    break;
                }
            }
        }
    }

    Ok((full_content, tool_calls))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_sse_line_content() {
        let line = r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.content_delta.as_deref(), Some("hello"));
        assert!(event.tool_call_delta.is_none());
    }

    #[test]
    fn test_parse_sse_line_done() {
        let line = "data: [DONE]";
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.finish_reason.as_deref(), Some("[DONE]"));
    }

    #[test]
    fn test_parse_sse_line_non_data() {
        assert!(parse_sse_line(": comment").is_none());
        assert!(parse_sse_line("").is_none());
    }

    #[test]
    fn test_parse_sse_line_tool_call_start() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"list_memos","arguments":""}}]}}]}"#;
        let event = parse_sse_line(line).unwrap();
        let tc = event.tool_call_delta.unwrap();
        assert_eq!(tc.index, 0);
        assert_eq!(tc.id.as_deref(), Some("call_abc"));
        assert_eq!(tc.function_name.as_deref(), Some("list_memos"));
    }

    #[test]
    fn test_parse_sse_line_tool_call_args_chunk() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"quer"}}]}}]}"#;
        let event = parse_sse_line(line).unwrap();
        let tc = event.tool_call_delta.unwrap();
        assert_eq!(tc.arguments_chunk.as_deref(), Some("{\"quer"));
        assert!(tc.id.is_none());
    }

    #[test]
    fn test_read_sse_stream_simple_content() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let mut chunks = Vec::new();
        let (content, tool_calls) = read_sse_stream(cursor, |c| chunks.push(c.to_string())).unwrap();
        assert_eq!(content, "Hi there");
        assert_eq!(chunks, vec!["Hi", " there"]);
        assert!(tool_calls.is_empty());
    }

    #[test]
    fn test_read_sse_stream_tool_calls_accumulated() {
        let input = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_memos\",\"arguments\":\"\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"Rust\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let (content, tool_calls) = read_sse_stream(cursor, |_| {}).unwrap();
        assert_eq!(content, "");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].name, "list_memos");
        assert_eq!(tool_calls[0].arguments, r#"{"query":"Rust"}"#);
    }

    #[test]
    fn test_read_sse_stream_mixed_content_and_tool_calls() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"让我查一下\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_tags\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let (content, tool_calls) = read_sse_stream(cursor, |_| {}).unwrap();
        assert_eq!(content, "让我查一下");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "list_tags");
    }
}
