//! Validation of one report section envelope (TS plan A4 `validateSection`, ADR-R3-4).
//!
//! Fatal errors drop the whole section: `wrong_section` (the envelope names another section, or
//! the requested part is null), `too_many_focuses` (more than [`MAX_FOCUSES`]) and `area_missing`
//! (the areas are not exactly the 7 distinct areas). Item errors drop one item: a singleton item
//! becomes `None`, an array entry is removed, and each drop is listed in `item_errors` with its
//! path (`weaknesses.items[1]`). Each paragraph goes through [`validate_text`] with the rules of
//! [`report_text_rules`]. Weakness, focus and opening items need a citation; strengths and areas
//! may have none, and then cannot use position tokens (`uncited_reference`).
//!
//! Model text outside an `Item` (a weakness `title`, a drill `rationale`) is shown too. It is
//! validated with the `Item` rules and may not hold tokens, since it carries no segments.

use std::collections::HashSet;

use super::evidence::Evidence;
use super::focus::GENERAL_FOCUS;
use super::language::{RuleKind, RuleMeta, report_text_rules};
use super::text::{TextRules, validate_text};
use crate::input::types::{AreaId, CoachingInput};
use crate::output::codes::{Citation, DroppedItem, SectionKey, ValidationCode, ValidationError};
use crate::output::drills::{DrillType, drills_for};
use crate::output::report::{
    AreasSection, ClimbPlanSection, Item, OpeningsSection, OverviewSection, ReportSectionEnvelope,
    ValidatedAreaItem, ValidatedAreasSection, ValidatedClimbPlanSection, ValidatedFocus,
    ValidatedItem, ValidatedOpeningItem, ValidatedOpeningsSection, ValidatedOverviewSection,
    ValidatedPart, ValidatedSection, ValidatedStrengthItem, ValidatedWeaknessItem,
    ValidatedWeaknessesSection, WeaknessesSection,
};
use crate::output::schema::MAX_FOCUSES;
use crate::segments::Segment;

/// What a section is validated against.
pub struct SectionContext<'a> {
    pub input: &'a CoachingInput,
    pub evidence: &'a Evidence,
    /// From [`super::focus::select_focuses`]; empty means the general focus.
    pub focuses: &'a [String],
}

/// The result of [`validate_section`]. `content` is `None` exactly when `fatal` is non-empty.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionValidation {
    pub content: Option<ValidatedSection>,
    pub item_errors: Vec<DroppedItem>,
    pub fatal: Vec<ValidationError>,
}

fn err(code: ValidationCode, path: &str, detail: impl Into<String>) -> ValidationError {
    ValidationError {
        code,
        path: path.to_owned(),
        detail: detail.into(),
    }
}

type Rules = Box<dyn Fn(RuleKind, RuleMeta) -> TextRules>;

struct V<'c, 'a> {
    ctx: &'c SectionContext<'a>,
    rules: Rules,
    section: SectionKey,
    dropped: Vec<DroppedItem>,
}

impl V<'_, '_> {
    fn drop_item(&mut self, path: &str, errors: Vec<ValidationError>) {
        self.dropped.push(DroppedItem {
            section: self.section.as_str().to_owned(),
            path: path.to_owned(),
            errors,
        });
    }

    fn text(
        &self,
        text: &str,
        citations: &[Citation],
        kind: RuleKind,
        meta: RuleMeta,
        path: &str,
    ) -> Result<Vec<Segment>, Vec<ValidationError>> {
        let rules = (self.rules)(kind, meta);
        validate_text(
            text,
            citations,
            self.ctx.evidence,
            &rules,
            &format!("{path}.text"),
        )
    }

    /// Model text that carries no segments: the `Item` rules, and no tokens.
    fn plain(&self, text: &str, path: &str) -> Vec<ValidationError> {
        let rules = (self.rules)(RuleKind::Item, RuleMeta::default());
        match validate_text(text, &[], self.ctx.evidence, &rules, path) {
            Err(e) => e,
            Ok(segs) if segs.iter().any(|s| !matches!(s, Segment::Text { .. })) => vec![err(
                ValidationCode::TokenSyntax,
                path,
                "tokens are not allowed here",
            )],
            Ok(_) => Vec::new(),
        }
    }

    fn item(
        &mut self,
        it: &Item,
        kind: RuleKind,
        meta: RuleMeta,
        path: &str,
    ) -> Option<ValidatedItem> {
        match self.text(&it.text, &it.citations, kind, meta, path) {
            Ok(segments) => Some(ValidatedItem {
                text: it.text.clone(),
                citations: it.citations.clone(),
                segments,
            }),
            Err(e) => {
                self.drop_item(path, e);
                None
            }
        }
    }

    fn overview(&mut self, o: &OverviewSection) -> ValidatedPart {
        let headline = self.item(
            &o.headline,
            RuleKind::Item,
            RuleMeta::default(),
            "overview.headline",
        );
        let summary = self.item(
            &o.summary,
            RuleKind::Summary,
            RuleMeta::default(),
            "overview.summary",
        );
        let mut strengths = Vec::new();
        for (i, s) in o.strengths.iter().enumerate() {
            let path = format!("overview.strengths[{i}]");
            let meta = RuleMeta {
                area: Some(s.area),
                ..RuleMeta::default()
            };
            match self.text(&s.text, &s.citations, RuleKind::Item, meta, &path) {
                Ok(segments) => strengths.push(ValidatedStrengthItem {
                    text: s.text.clone(),
                    citations: s.citations.clone(),
                    segments,
                    area: s.area,
                }),
                Err(e) => self.drop_item(&path, e),
            }
        }
        ValidatedPart::Overview(ValidatedOverviewSection {
            headline,
            summary,
            strengths,
        })
    }

    fn weaknesses(&mut self, w: &WeaknessesSection) -> ValidatedPart {
        let mut items = Vec::new();
        for (i, it) in w.items.iter().enumerate() {
            let path = format!("weaknesses.items[{i}]");
            if !self
                .ctx
                .input
                .weaknesses
                .iter()
                .any(|x| x.id == it.weakness_id)
            {
                let e = err(
                    ValidationCode::UnknownWeakness,
                    &format!("{path}.weaknessId"),
                    it.weakness_id.clone(),
                );
                self.drop_item(&path, vec![e]);
                continue;
            }
            let meta = RuleMeta {
                weakness_id: Some(it.weakness_id.clone()),
                ..RuleMeta::default()
            };
            let errors = match self.text(&it.text, &it.citations, RuleKind::Weakness, meta, &path) {
                Ok(segments) => {
                    let title = self.plain(&it.title, &format!("{path}.title"));
                    if title.is_empty() {
                        items.push(ValidatedWeaknessItem {
                            text: it.text.clone(),
                            citations: it.citations.clone(),
                            segments,
                            weakness_id: it.weakness_id.clone(),
                            title: it.title.clone(),
                        });
                        continue;
                    }
                    title
                }
                Err(e) => e,
            };
            self.drop_item(&path, errors);
        }
        ValidatedPart::Weaknesses(ValidatedWeaknessesSection { items })
    }

    fn openings(&mut self, o: &OpeningsSection) -> ValidatedPart {
        let mut items = Vec::new();
        for (i, it) in o.items.iter().enumerate() {
            let path = format!("openings.items[{i}]");
            let Some(line) = self
                .ctx
                .input
                .openings
                .iter()
                .find(|x| x.line_key == it.line_key)
            else {
                let e = err(
                    ValidationCode::UnknownLine,
                    &format!("{path}.lineKey"),
                    it.line_key.clone(),
                );
                self.drop_item(&path, vec![e]);
                continue;
            };
            if line.verdict != it.verdict {
                let e = err(
                    ValidationCode::VerdictMismatch,
                    &format!("{path}.verdict"),
                    format!(
                        "{} is computed as {}, not {}",
                        it.line_key, line.verdict, it.verdict
                    ),
                );
                self.drop_item(&path, vec![e]);
                continue;
            }
            let meta = RuleMeta {
                line_key: Some(it.line_key.clone()),
                ..RuleMeta::default()
            };
            match self.text(&it.text, &it.citations, RuleKind::Opening, meta, &path) {
                Ok(segments) => items.push(ValidatedOpeningItem {
                    text: it.text.clone(),
                    citations: it.citations.clone(),
                    segments,
                    line_key: it.line_key.clone(),
                    verdict: it.verdict,
                }),
                Err(e) => self.drop_item(&path, e),
            }
        }
        ValidatedPart::Openings(ValidatedOpeningsSection { items })
    }

    /// The drills a focus may use: `drills_for` its weakness (with its variant); the general
    /// focus may use only `own_game_analysis`, which every weakness but W12 allows.
    fn allowed_drills(&self, id: &str) -> Vec<DrillType> {
        if id == GENERAL_FOCUS {
            return vec![DrillType::OwnGameAnalysis];
        }
        let variant = self
            .ctx
            .input
            .weaknesses
            .iter()
            .find(|w| w.id == id)
            .and_then(|w| w.variant.as_deref());
        drills_for(id, variant)
    }

    fn climb_plan(&mut self, c: &ClimbPlanSection) -> ValidatedPart {
        let intro = self.item(
            &c.intro,
            RuleKind::Item,
            RuleMeta::default(),
            "climbPlan.intro",
        );
        let mut focuses = Vec::new();
        let mut seen = HashSet::new();
        for (i, f) in c.focuses.iter().enumerate() {
            let path = format!("climbPlan.focuses[{i}]");
            let id = f.weakness_id.as_str();
            let selected = if self.ctx.focuses.is_empty() {
                id == GENERAL_FOCUS
            } else {
                self.ctx.focuses.iter().any(|x| x == id)
            };
            let mut errors = Vec::new();
            if !selected || !seen.insert(id.to_owned()) {
                let why = if selected {
                    "a duplicate focus"
                } else {
                    "not a selected focus"
                };
                errors.push(err(
                    ValidationCode::FocusNotSelected,
                    &format!("{path}.weaknessId"),
                    format!("{id}: {why}"),
                ));
            }
            let allowed = self.allowed_drills(id);
            for (j, d) in f.drills.iter().enumerate() {
                let dp = format!("{path}.drills[{j}]");
                if !allowed.contains(&d.drill_type) {
                    errors.push(err(
                        ValidationCode::DrillNotAllowed,
                        &format!("{dp}.drillType"),
                        format!("{} for {id}", d.drill_type),
                    ));
                }
                errors.extend(self.plain(&d.rationale, &format!("{dp}.rationale")));
            }
            if !errors.is_empty() {
                self.drop_item(&path, errors);
                continue;
            }
            let meta = RuleMeta {
                weakness_id: Some(id.to_owned()),
                ..RuleMeta::default()
            };
            let why = self.item(&f.why, RuleKind::Focus, meta, &format!("{path}.why"));
            let habit = self.item(
                &f.habit,
                RuleKind::Item,
                RuleMeta::default(),
                &format!("{path}.habit"),
            );
            focuses.push(ValidatedFocus {
                weakness_id: f.weakness_id.clone(),
                why,
                drills: f.drills.clone(),
                habit,
            });
        }
        let weekly_rhythm = self.item(
            &c.weekly_rhythm,
            RuleKind::Item,
            RuleMeta::default(),
            "climbPlan.weeklyRhythm",
        );
        ValidatedPart::ClimbPlan(ValidatedClimbPlanSection {
            intro,
            focuses,
            weekly_rhythm,
        })
    }

    fn areas(&mut self, a: &AreasSection) -> ValidatedPart {
        let mut items = Vec::new();
        for (i, it) in a.items.iter().enumerate() {
            let path = format!("areas.items[{i}]");
            let meta = RuleMeta {
                area: Some(it.area),
                ..RuleMeta::default()
            };
            match self.text(&it.text, &it.citations, RuleKind::Area, meta, &path) {
                Ok(segments) => items.push(ValidatedAreaItem {
                    text: it.text.clone(),
                    citations: it.citations.clone(),
                    segments,
                    area: it.area,
                }),
                Err(e) => self.drop_item(&path, e),
            }
        }
        ValidatedPart::Areas(ValidatedAreasSection { items })
    }
}

fn fatal(e: ValidationError) -> SectionValidation {
    SectionValidation {
        content: None,
        item_errors: Vec::new(),
        fatal: vec![e],
    }
}

/// Validates the envelope Claude returned for the `requested` section.
pub fn validate_section(
    requested: SectionKey,
    env: &ReportSectionEnvelope,
    ctx: &SectionContext,
) -> SectionValidation {
    use ValidationCode as C;
    let present = match requested {
        SectionKey::Overview => env.overview.is_some(),
        SectionKey::Weaknesses => env.weaknesses.is_some(),
        SectionKey::Openings => env.openings.is_some(),
        SectionKey::ClimbPlan => env.climb_plan.is_some(),
        SectionKey::Areas => env.areas.is_some(),
    };
    if env.section != requested || !present {
        return fatal(err(
            C::WrongSection,
            "section",
            format!(
                "requested {requested}, got {} (part present: {present})",
                env.section
            ),
        ));
    }
    if let Some(c) = &env.climb_plan
        && requested == SectionKey::ClimbPlan
        && c.focuses.len() > MAX_FOCUSES
    {
        return fatal(err(
            C::TooManyFocuses,
            "climbPlan.focuses",
            format!("{} focuses, at most {MAX_FOCUSES}", c.focuses.len()),
        ));
    }
    if let Some(a) = &env.areas
        && requested == SectionKey::Areas
    {
        let distinct: HashSet<AreaId> = a.items.iter().map(|i| i.area).collect();
        if a.items.len() != AreaId::ALL.len() || distinct.len() != AreaId::ALL.len() {
            return fatal(err(
                C::AreaMissing,
                "areas.items",
                format!(
                    "{} items over {} distinct areas, need all {}",
                    a.items.len(),
                    distinct.len(),
                    AreaId::ALL.len()
                ),
            ));
        }
    }
    let mut v = V {
        ctx,
        rules: Box::new(report_text_rules(ctx.input)),
        section: requested,
        dropped: Vec::new(),
    };
    let part = match requested {
        SectionKey::Overview => env.overview.as_ref().map(|o| v.overview(o)),
        SectionKey::Weaknesses => env.weaknesses.as_ref().map(|w| v.weaknesses(w)),
        SectionKey::Openings => env.openings.as_ref().map(|o| v.openings(o)),
        SectionKey::ClimbPlan => env.climb_plan.as_ref().map(|c| v.climb_plan(c)),
        SectionKey::Areas => env.areas.as_ref().map(|a| v.areas(a)),
    };
    SectionValidation {
        content: part.map(ValidatedSection::new),
        item_errors: v.dropped,
        fatal: Vec::new(),
    }
}
