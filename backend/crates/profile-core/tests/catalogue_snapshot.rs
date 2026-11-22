//! The web's profile catalogue (M5b Task 1, ADR 0068) is a snapshot of the Rust one: weakness
//! labels, areas and minimum evidence, motif contract tags, and the KPI catalogue. This test
//! writes `backend/fixtures/profile/catalogue.json` with `UPDATE_SNAPSHOTS=1` and otherwise
//! fails when the Rust tables drift from it; the web's `catalogue.test.ts` pins its side.

use std::path::PathBuf;

use profile_core::constants::{RANKABLE, min_evidence, weakness_area, weakness_label};
use profile_core::kpi::catalogue::{KpiKind, kpi_catalogue};
use profile_core::types::{MotifTag, WeaknessKey};
use serde_json::{Value, json};

fn catalogue() -> Value {
    let weaknesses: Vec<Value> = WeaknessKey::ALL
        .iter()
        .map(|&k| {
            let (contract_id, variant) = k.contract_id();
            json!({
                "key": k.as_str(),
                "contractId": contract_id,
                "variant": variant,
                "label": weakness_label(k),
                "area": weakness_area(k).as_str(),
                "minEvidence": min_evidence(k),
                "rankable": RANKABLE.contains(&k),
            })
        })
        .collect();
    let motifs: Vec<Value> = MotifTag::ALL
        .iter()
        .map(|&t| json!({ "tag": t.as_str(), "contractTag": t.contract_tag() }))
        .collect();
    let kpis: Vec<Value> = kpi_catalogue()
        .iter()
        .map(|d| {
            json!({
                "metric": d.metric.to_string(),
                "area": d.area.map(|a| a.as_str()),
                "kind": match d.kind {
                    KpiKind::Mean => "mean",
                    KpiKind::Rate => "rate",
                    KpiKind::Last => "last",
                },
                "unit": d.contract_unit.map(|u| u.as_str()),
                "higherIsBetter": d.higher_is_better,
                "minN": d.min_n,
                "label": d.label,
            })
        })
        .collect();
    json!({ "weaknesses": weaknesses, "motifs": motifs, "kpis": kpis })
}

#[test]
fn catalogue_json_matches_the_fixture() {
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&catalogue()).expect("catalogue serialises")
    );
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/profile/catalogue.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixture dir");
        std::fs::write(&path, &text).expect("write fixture");
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run with UPDATE_SNAPSHOTS=1 to write it)",
            path.display()
        )
    });
    assert_eq!(
        stored.replace("\r\n", "\n"),
        text,
        "the profile catalogue changed; review it, rerun with UPDATE_SNAPSHOTS=1 and update the web"
    );
}
