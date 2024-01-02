// #![deny(clippy::all, clippy::pedantic, clippy::nursery)]
// #![deny(
//     missing_copy_implementations,
//     single_use_lifetimes,
//     variant_size_differences,
//     arithmetic_overflow,
//     missing_debug_implementations,
//     trivial_casts,
//     trivial_numeric_casts,
//     unused_import_braces,
//     unused_lifetimes,
//     unused_unsafe,
//     unused_tuple_struct_fields,
//     useless_ptr_null_checks,
//     cenum_impl_drop_cast,
//     while_true,
//     unused_features,
//     absolute_paths_not_starting_with_crate,
//     unused_allocation,
//     unreachable_code,
//     unused_comparisons,
//     unused_parens,
//     asm_sub_register,
//     break_with_label_and_loop,
//     bindings_with_variant_name,
//     anonymous_parameters
// )]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc, clippy::default_trait_access)]

#[macro_use]
pub mod macros;
pub mod error;

use std::sync::atomic::AtomicU64;

pub use error::NetworkTablesError;

pub mod client;
pub mod server;
pub(crate) mod spec;
pub(crate) mod bimap;
#[cfg(test)]
mod test;

pub use rmpv::{self, Value};

type WebSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub use error::*;

#[inline]
fn log_result<T, E: std::error::Error>(result: Result<T, E>) -> Result<T, E> {
    if let Err(err) = &result {
        tracing::error!("{err}");
    }
    result
}

static SEED: AtomicU64 = AtomicU64::new(0);

pub(crate) fn generate_uid() -> u64 {
    SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(frc)]
fn now() -> u64 {
    u64::try_from(frclib_core::time::uptime().as_micros())
        .expect("uptime is too large to fit in u64")
}
#[cfg(not(frc))]
fn now() -> u64 {
    u64::try_from(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before unix epoch")
        .as_micros())
    .expect("uptime is too large to fit in u64")
}