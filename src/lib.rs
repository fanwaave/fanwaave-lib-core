#![forbid(unsafe_code)]

pub mod config;
pub mod connection;
pub mod contact;
#[cfg(feature = "read-only")]
pub mod read_context;
pub mod error;
pub mod fanwaave_config;
pub mod fanwaave_flags2env;
pub mod flavor;
pub mod schema;

pub use config::CoreConfig;
pub use connection::CorePool;
#[cfg(feature = "read-only")]
pub use read_context::{ReadContext, ReadError};
pub use error::CoreError;
pub use flavor::DatabaseFlavor;
pub use schema::SCHEMA_REVISION;

pub mod embedding_contract;
pub mod embedding_orm_projection;
