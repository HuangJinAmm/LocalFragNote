//! IPC 命令模块汇总
//!
//! 每个子模块对应一个领域：memo、attachment、reaction、memo_relation、setting

pub mod ai_chat;
pub mod attachment;
pub mod document_summary;
pub mod import_export;
pub mod lan;
pub mod llm_runner;
pub mod memo;
pub mod memo_relation;
pub mod reaction;
pub mod review;
pub mod setting;
