//! Library target — 暴露 lan 模块供集成测试访问
//!
//! binary 仍由 main.rs 构建，此处仅为 `tests/` 下的集成测试提供 `memos_app::lan::*` 入口。
//! server.rs 依赖 state / file_storage / error 模块，需在此声明以供 library 编译。

pub mod embedding;
pub mod error;
pub mod file_storage;
pub mod lan;
pub mod llm_runner;
pub mod state;
