#![forbid(unsafe_code)]

pub mod config;
pub mod connection;
pub mod contact;
pub mod error;
pub mod fanwaave_config;
pub mod fanwaave_flags2env;
pub mod flavor;
#[cfg(feature = "read-only")]
pub mod read_context;
pub mod schema;

pub use config::CoreConfig;
pub use connection::CorePool;
pub use error::CoreError;
pub use flavor::DatabaseFlavor;
#[cfg(feature = "read-only")]
pub use read_context::{ReadContext, ReadError};
pub use schema::SCHEMA_REVISION;

pub mod embedding_contract;
pub mod embedding_orm_projection;
