// This file deals with the cache / state management for the updater.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::network::UpdateResponse;

pub struct PatchInfo {
    pub path: String,
    pub version: String,
    pub hash: String,
}

#[derive(Deserialize, Serialize, Default, Clone)]
struct Slot {
    path: String,
    version: String,
    hash: String,
}

// This struct is public, as callers can have a handle to it, but modifying
// anything inside should be done via the functions below.
#[derive(Deserialize, Serialize)]
pub struct UpdaterState {
    current_slot_index: usize,
    slots: Vec<Slot>,
    // Add file path or FD so modifying functions can save it to disk?
}

impl Default for UpdaterState {
    fn default() -> Self {
        Self {
            current_slot_index: 0,
            slots: Vec::new(),
        }
    }
}

pub fn load_state(cache_dir: &str) -> anyhow::Result<UpdaterState> {
    // Load UpdaterState from disk
    let path = Path::new(cache_dir).join("state.json");
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let state = serde_json::from_reader(reader)?;
    Ok(state)
}

pub fn save_state(state: &UpdaterState, cache_dir: &str) -> anyhow::Result<()> {
    // Save UpdaterState to disk
    std::fs::create_dir_all(cache_dir)?;
    let path = Path::new(cache_dir).join("state.json");
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &state)?;
    Ok(())
}

pub fn current_patch_internal(state: &UpdaterState) -> Option<PatchInfo> {
    // If there is no state, return None.
    if state.slots.is_empty() {
        return None;
    }
    let slot = &state.slots[state.current_slot_index];
    // Otherwise return the version info from the current slot.
    return Some(PatchInfo {
        path: slot.path.clone(),
        version: slot.version.clone(),
        hash: slot.hash.clone(),
    });
}

fn unused_slot(state: &UpdaterState) -> usize {
    // Assume we only use two slots and pick the one that's not current.
    if state.slots.is_empty() {
        return 0;
    }
    if state.current_slot_index == 0 {
        return 1;
    }
    return 0;
}

fn set_slot(state: &mut UpdaterState, index: usize, slot: Slot) {
    if state.slots.len() < index + 1 {
        // Make sure we're not filling with empty slots.
        assert!(state.slots.len() == index);
        state.slots.resize(index + 1, Slot::default());
    }
    // Set the given slot to the given version.
    state.slots[index] = slot
}

pub fn set_current_slot(state: &mut UpdaterState, index: usize) {
    state.current_slot_index = index;
    // This does not implicitly save the state, but maybe should?
}

pub fn download_file_to_path(url: &str, path: &PathBuf) -> anyhow::Result<()> {
    // Download the file at the given url to the given path.
    let client = reqwest::blocking::Client::new();
    let response = client.get(url).send()?;
    let mut bytes = response.bytes()?;

    // Ensure the download directory exists.
    std::fs::create_dir_all(path.parent().unwrap())?;

    let mut file = File::create(path)?;
    file.write_all(&mut bytes)?;
    Ok(())
}

pub fn download_into_unused_slot(
    cache_dir: &str,
    update_response: &UpdateResponse,
    state: &mut UpdaterState,
) -> anyhow::Result<usize> {
    // Download the new version into the unused slot.
    let slot_index = unused_slot(state);
    download_into_slot(cache_dir, update_response, state, slot_index)?;
    Ok(slot_index)
}

fn download_into_slot(
    cache_dir: &str,
    update_response: &UpdateResponse,
    state: &mut UpdaterState,
    slot_index: usize,
) -> anyhow::Result<()> {
    // Download the new version into the given slot.
    // TODO: Give it a name other than libapp.txt.
    let path = Path::new(cache_dir)
        .join(format!("slot_{}", slot_index))
        .join("libapp.txt");

    // TODO: Shouldn't crash on malformed response.
    let update = update_response.update.as_ref().unwrap();

    // We should download into a separate place and move into place.
    // That would allow us to check the hash before moving into place.
    // Would also allow the move/state update to be "atomic" or at least allow
    // us to carefully guard against state corruption.
    // Would also let us support when we need to allow the system to download for us (e.g. iOS).
    download_file_to_path(&update.download_url, &path)?;
    // Check the hash against the download?

    // Update the state to include the new version.
    set_slot(
        state,
        slot_index,
        Slot {
            path: path.to_str().unwrap().to_string(),
            version: update.version.clone(),
            hash: update.hash.clone(),
        },
    );
    save_state(&state, cache_dir)?;

    return Ok(());
}
