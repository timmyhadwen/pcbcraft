//! Turns an online card into the parameters of `library.place_online`: the
//! symbol, the footprint its own link names, the 3D model when one is kept on
//! this computer, and the source and licence notices.
//!
//! # Rules
//! - Runs on a worker; a frame never waits for it.
//! - A footprint or model that cannot be had does not stop the part: the
//!   symbol alone is still placed, and the notice list says what was read.
//! - Only parts of the open library carry the `Library/Name` sku this reads.
//!
//! # Dependencies
//! - crates/library/src/online_import.rs
//! - crates/library/src/open_library.rs
//! - crates/engine/src/online.rs (the command that takes the result)

use pcbcraft_library::cad::{CadKind, CadRef, CadSource};
use pcbcraft_library::online_import::{footprint_ref_from_field, import_part};
use pcbcraft_library::open_library::OpenLibrarySource;
use pcbcraft_library::read_symbols;
use serde_json::Value;

/// Fetch one open-library part named by its `Library/Name` sku and shape it
/// for the engine.
///
/// # Rules
/// - Tries symbol + footprint + model, then without the model, then the symbol alone.
///
/// # Do not
/// - Write to the project; the caller runs `library.place_online` with the result.
pub(crate) fn import_open(source: &OpenLibrarySource, sku: &str) -> Result<Value, String> {
    let (library, name) = sku
        .split_once('/')
        .ok_or_else(|| "this part has no library path to open".to_owned())?;
    let symbol = CadRef::new(CadKind::Symbol, library, name).map_err(|e| e.to_string())?;
    let file = source.fetch_symbol(&symbol).map_err(|e| e.to_string())?;
    let text = file.text().map_err(|e| e.to_string())?;
    let footprint = read_symbols(text)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.name == name)
        .and_then(|d| footprint_ref_from_field(&d.footprint));
    let model = footprint
        .as_ref()
        .and_then(|f| CadRef::new(CadKind::Model, &f.library, &f.name).ok());
    let attempts = [
        (footprint.as_ref(), model.as_ref()),
        (footprint.as_ref(), None),
        (None, None),
    ];
    let mut last = String::new();
    for (fp, m) in attempts {
        match import_part(source, &symbol, fp, m) {
            Ok(part) => return Ok(part.to_params()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}
