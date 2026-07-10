//! 公共类型

use serde::{Deserialize, Serialize};

/// 行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RowStatus {
    Normal,
    Archived,
}

impl Default for RowStatus {
    fn default() -> Self {
        Self::Normal
    }
}

impl std::fmt::Display for RowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "NORMAL"),
            Self::Archived => write!(f, "ARCHIVED"),
        }
    }
}

impl rusqlite::ToSql for RowStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.to_string().into())
    }
}

impl rusqlite::types::FromSql for RowStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        match s {
            "NORMAL" => Ok(Self::Normal),
            "ARCHIVED" => Ok(Self::Archived),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

/// 可见性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Private
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "PUBLIC"),
            Self::Protected => write!(f, "PROTECTED"),
            Self::Private => write!(f, "PRIVATE"),
        }
    }
}

impl rusqlite::ToSql for Visibility {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.to_string().into())
    }
}

impl rusqlite::types::FromSql for Visibility {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        match s {
            "PUBLIC" => Ok(Self::Public),
            "PROTECTED" => Ok(Self::Protected),
            "PRIVATE" => Ok(Self::Private),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

/// 笔记关系类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MemoRelationType {
    Reference,
    Comment,
}

impl std::fmt::Display for MemoRelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reference => write!(f, "REFERENCE"),
            Self::Comment => write!(f, "COMMENT"),
        }
    }
}

impl rusqlite::ToSql for MemoRelationType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.to_string().into())
    }
}

impl rusqlite::types::FromSql for MemoRelationType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        match s {
            "REFERENCE" => Ok(Self::Reference),
            "COMMENT" => Ok(Self::Comment),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

/// 校验 UID 格式（字母数字下划线，1-64 字符）
pub fn validate_uid(uid: &str) -> bool {
    !uid.is_empty()
        && uid.len() <= 64
        && uid.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
