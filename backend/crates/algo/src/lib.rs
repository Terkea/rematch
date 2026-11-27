//! The algorithm manifest (ADRs 0104 and 0105).
//!
//! [`build`] gathers every component that shapes layers 1 to 3 (facts, profile, recommendation)
//! and the layer-4 prompt sets and models: the constants by import, the TS constants and the
//! Dockerfile pins by reading their files, and the constants with no version name of their own
//! as sha256 digests of the files that hold them. [`Manifest::id`] is
//! `algo-<version>+<first 12 hex of the sha256 of the canonical JSON>`.
//!
//! `cli algo manifest --write` writes it to `algorithm/manifest.json`; the tests in
//! `tests/manifest.rs` fail when the committed file differs. Writers read the committed id from
//! `common::algorithm`, never from this crate.

pub mod ban;
pub mod diff;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The algorithm's version, for people (the hash is the proof). Bump it by the policy in
/// `algorithm/CHANGELOG.md`: major when layer 1 to 3 output can change, minor when only the
/// language layer changes, patch when nothing a replay can see changes.
pub const VERSION: &str = "5.0.1";

/// The manifest's path in the repository.
pub const MANIFEST_PATH: &str = "algorithm/manifest.json";

/// The repository root this crate was compiled in (`backend/crates/algo/../../..`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A manifest: the version and the components (`to_value` adds the id). Serialises as
/// `{version, components}`; [`Manifest::to_value`] is the file's form.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub version: String,
    pub components: Value,
}

/// Canonical JSON: sorted keys (serde_json's map is a `BTreeMap`), no whitespace.
pub fn canonical(v: &Value) -> String {
    v.to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl Manifest {
    /// The hashed content: the version and the components.
    pub fn hashed(&self) -> Value {
        json!({ "version": self.version, "components": self.components })
    }

    pub fn hash(&self) -> String {
        sha256_hex(canonical(&self.hashed()).as_bytes())
    }

    /// `algo-<version>+<12 hex>`.
    pub fn id(&self) -> String {
        format!("algo-{}+{}", self.version, &self.hash()[..12])
    }

    /// The file's JSON: the id, the version and the components.
    pub fn to_value(&self) -> Value {
        json!({ "id": self.id(), "version": self.version, "components": self.components })
    }

    /// The committed form: two-space pretty JSON with sorted keys and a final newline.
    pub fn to_pretty(&self) -> String {
        let mut s = serde_json::to_string_pretty(&self.to_value()).unwrap_or_default();
        s.push('\n');
        s
    }

    /// A manifest file's JSON back into a manifest. The stored id must match its content.
    pub fn from_value(v: &Value) -> anyhow::Result<Manifest> {
        let version = v
            .get("version")
            .and_then(Value::as_str)
            .context("manifest has no version")?
            .to_owned();
        let components = v
            .get("components")
            .cloned()
            .context("manifest has no components")?;
        let m = Manifest {
            version,
            components,
        };
        if let Some(id) = v.get("id").and_then(Value::as_str)
            && id != m.id()
        {
            bail!("manifest id {id} does not match its content ({})", m.id());
        }
        Ok(m)
    }
}

/// A file's sha256 with CRLF normalised to LF, so a Windows checkout digests like Linux.
pub fn file_digest(root: &Path, rel: &str) -> anyhow::Result<String> {
    let bytes = std::fs::read(root.join(rel)).with_context(|| format!("reading {rel}"))?;
    Ok(sha256_hex(&normalise(&bytes)))
}

fn normalise(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Every file under `rel` (relative to `root`), sorted by path.
pub fn files_under(root: &Path, rel: &str, out: &mut Vec<String>) -> anyhow::Result<()> {
    let dir = root.join(rel);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("listing {rel}"))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let child = format!("{rel}/{name}");
        if e.file_type()?.is_dir() {
            files_under(root, &child, out)?;
        } else {
            out.push(child);
        }
    }
    Ok(())
}

/// A directory's digest: the sha256 of `<path> <sha256>\n` for every file under it, sorted by
/// path. A file is digested as itself.
pub fn path_digest(root: &Path, rel: &str) -> anyhow::Result<String> {
    if root.join(rel).is_file() {
        return file_digest(root, rel);
    }
    let mut files = Vec::new();
    files_under(root, rel, &mut files)?;
    if files.is_empty() {
        bail!("{rel} has no files");
    }
    let mut listing = String::new();
    for f in &files {
        listing.push_str(&format!("{f} {}\n", file_digest(root, f)?));
    }
    Ok(sha256_hex(listing.as_bytes()))
}

fn digests(root: &Path, paths: &[&str]) -> anyhow::Result<Value> {
    let mut m = BTreeMap::new();
    for p in paths {
        m.insert((*p).to_owned(), Value::String(path_digest(root, p)?));
    }
    Ok(serde_json::to_value(m)?)
}

/// The text of `export const NAME = <value>;` in a TS file (quotes kept off a string).
pub fn ts_const(root: &Path, rel: &str, name: &str) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(root.join(rel)).with_context(|| format!("reading {rel}"))?;
    let re = regex::Regex::new(&format!(r#"(?m)^export const {name} = "?([^";]+)"?;"#))?;
    re.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .with_context(|| format!("{rel} has no `export const {name}`"))
}

/// The default of `ARG NAME=<value>` in a Dockerfile.
pub fn docker_arg(root: &Path, rel: &str, name: &str) -> anyhow::Result<String> {
    dockerfile_value(root, rel, &format!("ARG {name}="))
}

fn dockerfile_value(root: &Path, rel: &str, prefix: &str) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(root.join(rel)).with_context(|| format!("reading {rel}"))?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix(prefix))
        .map(|v| v.trim().trim_end_matches('\\').trim().to_owned())
        .with_context(|| format!("{rel} has no `{prefix}`"))
}

fn stockfish_release(root: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(root.join("backend/Dockerfile"))?;
    let re = regex::Regex::new(r"releases/download/([A-Za-z0-9_.-]+)/")?;
    re.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .context("backend/Dockerfile names no Stockfish release")
}

fn maia_package(root: &Path) -> anyhow::Result<String> {
    let rel = "services/maia/pyproject.toml";
    let text = std::fs::read_to_string(root.join(rel))?;
    let re = regex::Regex::new(r#""(maia2==[^"]+)""#)?;
    re.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .with_context(|| format!("{rel} pins no maia2"))
}

fn python_int(root: &Path, rel: &str, name: &str) -> anyhow::Result<i64> {
    let text = std::fs::read_to_string(root.join(rel))?;
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{name} = ")))
        .and_then(|v| v.trim().parse().ok())
        .with_context(|| format!("{rel} has no `{name} = <int>`"))
}

/// The TS files whose rules the web owns (checked again by `apps/web/test/algorithm.test.ts`).
pub const TS_TRAINER: &str = "apps/web/src/server/train/model.ts";
pub const TS_FOCUS: &str = "apps/web/src/server/focus/goal.ts";
/// The free-capture check the trainer's `keepCandidate` calls (a pure module since #125).
pub const TS_FREE_CAPTURE: &str = "apps/web/src/server/profile/obvious-capture.ts";
pub const TS_GAME_STATS: &str = "apps/web/src/server/stats/game-stats.ts";

/// Builds the manifest of the code under `root` (the repository root).
pub fn build(root: &Path) -> anyhow::Result<Manifest> {
    use ai::client::model::AiModel;
    use ai::prompts::{
        CHANGE_PROMPT_SET, CHAT_PROMPT_SET, CHAT_SUMMARY_PROMPT_SET, REPORT_PROMPT_SET,
        WEEKLY_PROMPT_SET, prompt_set_hash,
    };
    use enrich::protocol as enr;
    use profile_core::stats::priority as rank;

    let facts = json!({
        "engine_protocol": engine::ANALYSIS_PROTOCOL,
        "engine_nodes": engine::ENGINE_NODES,
        "engine_options": {
            "threads": engine::ENGINE_THREADS,
            "hash_mb": engine::ENGINE_HASH_MB,
            "new_game": "ucinewgame before every search",
            "limit": "go nodes",
        },
        "stockfish": {
            "release": stockfish_release(root)?,
            "asset": docker_arg(root, "backend/Dockerfile", "STOCKFISH_ASSET")?,
            "sha256": docker_arg(root, "backend/Dockerfile", "STOCKFISH_SHA256")?,
        },
        "enrichment": {
            "protocol": enr::ENRICHMENT_PROTOCOL,
            "multipv_protocol": enr::MULTIPV_PROTOCOL,
            "multipv": enr::MULTIPV,
            "multipv_nodes": enr::MULTIPV_NODES,
            "threat_protocol": enr::THREAT_PROTOCOL,
            "threat_nodes": enr::THREAT_NODES,
            "max_critical_plies": enr::MAX_CRITICAL_PLIES,
        },
        "syzygy": {
            "set": "3-4-5",
            "manifest_sha256": file_digest(root, "backend/crates/lichess-db/data/syzygy-345.sha256")?,
        },
        "maia": {
            "package": maia_package(root)?,
            "default_model": dockerfile_value(root, "services/maia/Dockerfile", "MAIA_MODEL=")?,
            "elo_min": python_int(root, "services/maia/app/model.py", "ELO_MIN")?,
            "elo_max": python_int(root, "services/maia/app/model.py", "ELO_MAX")?,
            "used_when": "MAIA_URL is configured",
        },
        "player_game_metrics_version": profile_core::types::PLAYER_GAME_METRICS_VERSION,
        "phase_metrics_version": ts_const(root, TS_GAME_STATS, "PHASE_METRICS_VERSION")?.parse::<u32>()?,
        "source_digest": digests(root, &[
            "backend/crates/chess-core/src",
            "backend/crates/engine/src",
            "backend/crates/analysis/src",
            "backend/crates/enrich/src",
        ])?,
    });
    let profile = json!({
        "detector_version": profile_core::timeline::states::DETECTOR_VERSION,
        "decisive_dw": profile_core::weakness::detectors::DECISIVE_DW,
        "t_win": profile_core::weakness::observe_game::T_WIN,
        "peer_rating_window": profile::peer_benchmark::PEER_RATING_WINDOW,
        // The motif tags a player is told about (#122, ADR 0112): every tag is still computed.
        "player_visible_motifs": {
            "source": coaching_input::motifs::PLAYER_VISIBLE_MOTIFS_SOURCE,
            "tags": coaching_input::motifs::PLAYER_VISIBLE_MOTIFS,
        },
        // The trajectory's rating verdict, stated or withheld (#136, ADR 0116): always computed.
        "trajectory_rating_verdict": {
            "source": coaching_input::trajectory::RATING_VERDICT_SOURCE,
            "shown": coaching_input::trajectory::RATING_VERDICT_SHOWN,
        },
        "source_digest": digests(root, &[
            "backend/crates/profile-core/src",
            "backend/crates/profile/src",
        ])?,
    });
    let recommendation = json!({
        "ranking_version": rank::RANKING_VERSION,
        "rank_min_excess_per_game": rank::RANK_MIN_EXCESS_PER_GAME,
        "model_loss_per_winning_episode": rank::MODEL_LOSS_PER_WINNING_EPISODE,
        "min_games": rank::MIN_GAMES,
        "coaching_input_version": coaching_input::limits::INPUT_VERSION,
        "weekly_input_version": profile::coaching::weekly::WEEKLY_INPUT_VERSION,
        "trainer_version": ts_const(root, TS_TRAINER, "TRAINER_VERSION")?,
        "focus_version": ts_const(root, TS_FOCUS, "FOCUS_VERSION")?,
        "source_digest": digests(root, &[
            "backend/crates/coaching-input/src",
            "backend/crates/ai/src/output/drills.rs",
            TS_TRAINER,
            TS_FOCUS,
            TS_FREE_CAPTURE,
        ])?,
    });
    let mut models: Vec<&str> = AiModel::ALL.iter().map(|m| m.as_str()).collect();
    models.sort_unstable();
    let language = json!({
        "prompt_sets": {
            "report": prompt_set_hash(REPORT_PROMPT_SET),
            "chat": prompt_set_hash(CHAT_PROMPT_SET),
            "chat_summary": prompt_set_hash(CHAT_SUMMARY_PROMPT_SET),
            "weekly": prompt_set_hash(WEEKLY_PROMPT_SET),
            "change": prompt_set_hash(CHANGE_PROMPT_SET),
        },
        "models": models,
        "anthropic_version": ai::client::wire::ANTHROPIC_VERSION,
        "validator_digest": path_digest(root, "backend/crates/ai/src/validator")?,
    });
    Ok(Manifest {
        version: VERSION.to_owned(),
        components: integral(json!({
            "facts": facts,
            "profile": profile,
            "recommendation": recommendation,
            "language": language,
        })),
    })
}

/// Writes a whole-number float as an integer (`15.0` as `15`), so the canonical JSON is the
/// same in Rust and in JavaScript (`JSON.stringify`), and `scripts/algorithm/check.mjs` can
/// recompute the id.
pub fn integral(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, integral(v))).collect()),
        Value::Array(a) => Value::Array(a.into_iter().map(integral).collect()),
        Value::Number(n) if n.is_f64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Value::from(f as i64),
            _ => Value::Number(n),
        },
        other => other,
    }
}

/// Reads a manifest file.
pub fn read(path: &Path) -> anyhow::Result<Manifest> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Manifest::from_value(&serde_json::from_str(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_the_version_and_twelve_hex_of_the_canonical_json() {
        let m = Manifest {
            version: "1.0.0".to_owned(),
            components: json!({ "b": 1, "a": { "z": "x", "y": [1, 2] } }),
        };
        assert_eq!(
            canonical(&m.hashed()),
            r#"{"components":{"a":{"y":[1,2],"z":"x"},"b":1},"version":"1.0.0"}"#
        );
        // sha256 of the line above, pinned.
        assert_eq!(m.id(), format!("algo-1.0.0+{}", &m.hash()[..12]));
        assert_eq!(m.id(), "algo-1.0.0+d34c96e0b472");
    }

    #[test]
    fn whole_floats_are_written_as_integers() {
        assert_eq!(
            canonical(&integral(json!({ "a": 15.0, "b": 0.15, "c": [2.0] }))),
            r#"{"a":15,"b":0.15,"c":[2]}"#
        );
    }

    #[test]
    fn crlf_and_lf_digest_the_same() {
        assert_eq!(normalise(b"a\r\nb\n"), b"a\nb\n");
        assert_eq!(normalise(b"a\rb"), b"a\rb");
    }

    #[test]
    fn the_pretty_form_round_trips_and_common_reads_its_id() {
        let m = Manifest {
            version: "2.1.0".to_owned(),
            components: json!({ "facts": { "id": "nested" } }),
        };
        let text = m.to_pretty();
        assert_eq!(common::algorithm::id_of(&text), Some(m.id().as_str()));
        let back = Manifest::from_value(&serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn a_tampered_id_is_refused() {
        let v = json!({ "id": "algo-1.0.0+000000000000", "version": "1.0.0", "components": {} });
        assert!(Manifest::from_value(&v).is_err());
    }
}
