//! Library target — 暴露 lan 模块供集成测试访问
//!
//! binary 仍由 main.rs 构建，此处仅为 `tests/` 下的集成测试提供 `memos_app::lan::*` 入口。

pub mod lan;
