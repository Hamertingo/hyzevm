//! HyzeVM — uma máquina Linux própria para cada agente.
//! A lib concentra tudo (CLI, MCP e app usam as mesmas funções).
pub mod browser;
pub mod desktop;
pub mod doctor;
pub mod mcp;
pub mod ops;
pub mod ask;
pub mod qemu;
pub mod service;
pub mod share;
pub mod ssh;
pub mod store;
pub mod takeover;
pub mod template;
pub mod ui;
pub mod update;
pub mod vault;
pub mod watch;
