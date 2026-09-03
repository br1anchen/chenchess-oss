//! Slot markers: the Language Layer's grounding mechanism for free-form prose.
//!
//! A Language Layer writes commentary in its own words and names every fact
//! through a typed marker — `{betterMove}`, `{bestEval}` — never a figure of
//! its own. The runtime substitutes the canonical rendering after the gate
//! passes, so every number a Player reads was rendered here.
//!
//! That inverts the guarantee the positional skeleton gave. A `contains`
//! check proves the right fact is *present* and says nothing about the invented
//! `+0.9` sitting beside it; markers make a wrong fact *inexpressible*, because
//! bare figures are rejected outright.
//!
//! The markers used are also the claims asserted, which is what gives the
//! grounding ledger something on the other side of its equals sign.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerViolation {
    /// A marker this task does not define, or a stray brace. Either way
    /// substitution would leave the Player reading our internal syntax.
    ///
    /// The offending text is the model's, not ours, so it is not carried.
    UnknownMarker,
    /// One fact rendered twice reads as a stutter. A form that names rather
    /// than claims is exempt — see [`MarkerForm::repeatable`].
    RepeatedMarker(&'static str),
    MissingRequiredMarker(&'static str),
    /// A figure the model wrote itself instead of naming through a marker.
    BareFigure,
    /// A marker with no slot-invariant form used inside a longer sentence.
    /// Only [`MarkerForm::OwnSentence`] can raise this.
    MisplacedMarker(&'static str),
}

/// How a rendering fits the slot the model put it in.
///
/// A slot is a clause and some facts are sentences, so one canonical string per
/// marker cannot be right everywhere: obeying the prompt produced "You played
/// e4, and After e4, the evaluation is +0.5 — Slightly better for White.
/// because it was sound". The runtime fits the rendering to the seam rather
/// than asking the model to fit its sentence to the rendering, which is the
/// same move as grounding producing the text instead of approving it.
///
/// Re-casing is the dangerous operation and it is confined to [`Self::Anywhere`]
/// on purpose. `e4` capitalised at a sentence start is `E4`, which is not the
/// move; "White delivered checkmate" downcased is not the colour. Neither is
/// reachable here, because notation is [`Self::Literal`] and nothing is ever
/// downcased at all — the lowercase form of a sentence-shaped rendering is
/// authored beside it rather than derived from it.
#[derive(Debug, Clone)]
pub(crate) enum MarkerForm {
    /// Chess notation, substituted byte for byte in every position.
    Literal(String),
    /// Prose that reads in any slot, capitalised when it opens a sentence.
    Anywhere(String),
    /// A fact whose natural rendering is a whole sentence. Both forms are
    /// authored: `{difficulty}` drops its demonstrative subject between them
    /// ("This was especially difficult…" / "especially difficult…"), so the
    /// clause form is a rewrite rather than a transformation.
    Shaped { sentence: String, clause: String },
    /// A fact with no form that fits every frame. `{achievement}` renders a
    /// subjectless verb phrase, and models put it in noun slots ("You found
    /// secured checkmate"), infinitive slots ("the opportunity to secured
    /// checkmate") and bare fragments alike — four frames, no common shape. It
    /// may only stand as its own sentence, and misplacement is a rejection
    /// rather than prose the Player has to read.
    OwnSentence(String),
}

impl MarkerForm {
    /// The form the model is shown, which is the form it should write around:
    /// the clause for markers it will embed, the sentence for the one marker it
    /// must not.
    pub(crate) fn offered(&self) -> &str {
        match self {
            Self::Literal(rendering) | Self::Anywhere(rendering) => rendering,
            Self::Shaped { clause, .. } => clause,
            Self::OwnSentence(sentence) => sentence,
        }
    }

    /// Whether the prompt must tell the model this marker takes a sentence of
    /// its own.
    pub(crate) fn requires_own_sentence(&self) -> bool {
        matches!(self, Self::OwnSentence(_))
    }

    /// Whether a second use of this marker is reference rather than a second
    /// claim.
    ///
    /// Notation names a move, and naming the same move again is how the
    /// sentence wants to go: "You played h3 aiming to hold the centre, but
    /// after h3 the evaluation…". Every other form states a fact, and a fact
    /// said twice stutters.
    fn repeatable(&self) -> bool {
        match self {
            Self::Literal(_) => true,
            Self::Anywhere(_) | Self::Shaped { .. } | Self::OwnSentence(_) => false,
        }
    }

    /// Whether this form's rendering supplies its own article, so a
    /// model-written article directly before the marker would double it —
    /// "with an {evaluationLoss}" meeting "a 0.2-pawn margin" reads "with an
    /// a 0.2-pawn margin". Notation never absorbs: "the {likelyReply}" is the
    /// model's article doing real work in front of a bare move.
    fn absorbs_article(&self) -> bool {
        match self {
            Self::Literal(_) | Self::OwnSentence(_) => false,
            Self::Anywhere(rendering) => starts_with_article(rendering),
            Self::Shaped { clause, .. } => starts_with_article(clause),
        }
    }

    /// The rendering for one occurrence, given how the model framed it.
    ///
    /// `None` is the single way this fails — an own-sentence rendering the
    /// model framed inside a longer sentence. The caller holds the marker's
    /// name, so the caller names the violation.
    fn fitted(&self, seam: Seam) -> Option<String> {
        Some(match self {
            Self::Literal(rendering) => rendering.clone(),
            Self::Anywhere(rendering) => seam.open_sentence(rendering),
            Self::Shaped { sentence, clause } => {
                if seam.standalone {
                    sentence.clone()
                } else {
                    seam.open_sentence(clause)
                }
            }
            Self::OwnSentence(sentence) => {
                if !seam.standalone {
                    return None;
                }
                sentence.clone()
            }
        })
    }
}

/// Where one marker occurrence sits in the model's sentence.
#[derive(Debug, Clone, Copy)]
struct Seam {
    initial: bool,
    /// Opens a sentence *and* closes it, so a sentence-shaped rendering reads
    /// as the model intended rather than as an interruption.
    standalone: bool,
}

impl Seam {
    /// Reads the seam from the prose either side of the marker.
    ///
    /// Standalone is decided by whether the model kept writing the *same*
    /// sentence, not by whether it punctuated the end of one. Models routinely
    /// leave the full stop to the rendering — "…and bishop. {observation} The
    /// move is consistent…" — so requiring their punctuation would take the
    /// clause form here and swallow the sentence break the prose depends on.
    fn read(before: &str, after: &str) -> Self {
        let initial = before
            .trim_end()
            .chars()
            .next_back()
            .is_none_or(|character| matches!(character, '.' | '!' | '?'));
        Self {
            initial,
            standalone: initial && !continues_clause(after),
        }
    }

    fn open_sentence(&self, rendering: &str) -> String {
        if !self.initial {
            return rendering.to_string();
        }
        let mut characters = rendering.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().chain(characters).collect(),
            None => String::new(),
        }
    }
}

/// One task's markers and their canonical renderings.
///
/// Renderings are Player-facing text produced from the recorded facts, so the
/// vocabulary is rebuilt per moment. Its *shape* — the names, which are
/// required, and how each renders — joins the prompt digest, because changing
/// how `{bestEval}` reads changes what the Player sees without changing a byte
/// of model output.
#[derive(Debug, Default, Clone)]
pub(crate) struct MarkerVocabulary {
    renderings: BTreeMap<&'static str, MarkerForm>,
    required: Vec<&'static str>,
}

/// Prose that passed the marker gate, in the two forms the rest of the gate
/// needs: what the Player will read, and what the model actually wrote.
pub(crate) struct GroundedProse {
    /// Markers substituted. This is the comment.
    pub(crate) text: String,
    /// Markers elided. Chess literals and bare figures are judged against
    /// this, so canonical renderings are never mistaken for model claims.
    pub(crate) authored: String,
    pub(crate) markers: Vec<&'static str>,
}

impl MarkerVocabulary {
    /// Prose that fits any slot.
    pub(crate) fn require(&mut self, marker: &'static str, rendering: impl Into<String>) {
        self.require_form(marker, MarkerForm::Anywhere(rendering.into()));
    }

    /// Chess notation, never re-cased.
    pub(crate) fn require_literal(&mut self, marker: &'static str, rendering: impl Into<String>) {
        self.require_form(marker, MarkerForm::Literal(rendering.into()));
    }

    /// A fact whose natural rendering is a sentence, with the clause it becomes
    /// inside one.
    pub(crate) fn require_shaped(
        &mut self,
        marker: &'static str,
        sentence: impl Into<String>,
        clause: impl Into<String>,
    ) {
        self.require_form(
            marker,
            MarkerForm::Shaped {
                sentence: sentence.into(),
                clause: clause.into(),
            },
        );
    }

    /// A fact that may only stand as its own sentence.
    pub(crate) fn require_own_sentence(
        &mut self,
        marker: &'static str,
        sentence: impl Into<String>,
    ) {
        self.require_form(marker, MarkerForm::OwnSentence(sentence.into()));
    }

    pub(crate) fn require_form(&mut self, marker: &'static str, form: MarkerForm) {
        self.renderings.insert(marker, form);
        self.required.push(marker);
    }

    pub(crate) fn offer(&mut self, marker: &'static str, rendering: impl Into<String>) {
        self.renderings
            .insert(marker, MarkerForm::Anywhere(rendering.into()));
    }

    pub(crate) fn offer_literal(&mut self, marker: &'static str, rendering: impl Into<String>) {
        self.renderings
            .insert(marker, MarkerForm::Literal(rendering.into()));
    }

    /// Offers a marker only when the facts carry it. An absent optional fact
    /// leaves the marker undefined, so naming it is an unknown marker rather
    /// than a silently empty substitution.
    pub(crate) fn offer_available(&mut self, marker: &'static str, rendering: Option<String>) {
        if let Some(rendering) = rendering {
            self.offer(marker, rendering);
        }
    }

    /// The literal counterpart of [`Self::offer_available`], for notation the
    /// facts may not carry.
    pub(crate) fn offer_literal_available(
        &mut self,
        marker: &'static str,
        rendering: Option<String>,
    ) {
        if let Some(rendering) = rendering {
            self.offer_literal(marker, rendering);
        }
    }

    pub(crate) fn required_markers(&self) -> &[&'static str] {
        &self.required
    }

    /// Every marker this moment defines, with the rendering the model is shown
    /// and whether it is required. The prompt shows the model exactly this, so
    /// the vocabulary the gate enforces and the vocabulary the model is offered
    /// cannot drift apart.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&'static str, &MarkerForm, bool)> {
        self.renderings
            .iter()
            .map(|(marker, form)| (*marker, form, self.required.contains(marker)))
    }

    /// Parses, checks and substitutes in one pass — steps 1 to 3 and 5 of the
    /// gate. Step 4 runs against [`GroundedProse::authored`].
    pub(crate) fn ground(&self, text: &str) -> Result<GroundedProse, MarkerViolation> {
        let mut substituted = String::with_capacity(text.len());
        let mut authored = String::with_capacity(text.len());
        let mut markers: Vec<&'static str> = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let (before, tail) = rest.split_at(open);
            substituted.push_str(before);
            authored.push_str(before);
            if before.contains('}') {
                return Err(MarkerViolation::UnknownMarker);
            }
            let close = tail.find('}').ok_or(MarkerViolation::UnknownMarker)?;
            let name = &tail[1..close];
            let (marker, form) = self
                .renderings
                .get_key_value(name)
                .ok_or(MarkerViolation::UnknownMarker)?;
            if !markers.contains(marker) {
                markers.push(marker);
            } else if !form.repeatable() {
                return Err(MarkerViolation::RepeatedMarker(marker));
            }
            let mut following = &tail[close + 1..];
            if form.absorbs_article() {
                // One of the two articles goes, exactly as one of two full
                // stops does at the other end of the rendering. Stripped before
                // the seam is read, so an article that opened the model's
                // sentence hands sentence-initial position to the rendering.
                strip_trailing_article(&mut substituted);
            }
            // The seam is read against what has already been substituted, so a
            // marker abutting another marker sees the rendered text rather than
            // a brace.
            let seam = Seam::read(&substituted, following);
            let mut fitted = form
                .fitted(seam)
                .ok_or(MarkerViolation::MisplacedMarker(marker))?;
            if ends_sentence(&fitted) {
                // The model punctuating after a rendering that already ends in
                // a full stop is where "…for White.." came from. One of the two
                // goes.
                following = following.trim_start_matches(['.', '!', '?']);
            } else if !seam.standalone && opens_new_sentence(following) {
                // The mirror case: the model ended its sentence on the marker
                // and left the stop to the rendering, which the clause form no
                // longer carries. "…calculate e4 first By choosing this path…"
                // is that gap, and the runtime owns this seam too.
                fitted.push('.');
            }
            substituted.push_str(&fitted);
            // Markers stand in for facts the model never wrote, so the authored
            // form keeps a separator rather than joining the words around them.
            authored.push(' ');
            rest = following;
        }
        if rest.contains('}') {
            return Err(MarkerViolation::UnknownMarker);
        }
        substituted.push_str(rest);
        authored.push_str(rest);

        if let Some(missing) = self
            .required
            .iter()
            .copied()
            .find(|marker| !markers.contains(marker))
        {
            return Err(MarkerViolation::MissingRequiredMarker(missing));
        }
        if contains_bare_figure(&authored) {
            return Err(MarkerViolation::BareFigure);
        }
        Ok(GroundedProse {
            text: substituted,
            authored,
            markers,
        })
    }
}

/// Whether the model's next words carry on the sentence the marker sits in.
///
/// A lowercase word or a mid-sentence mark continues it; anything else — a
/// capital, another marker, the end of the paragraph — starts something new.
fn continues_clause(after: &str) -> bool {
    after
        .trim_start()
        .chars()
        .next()
        .is_some_and(|character| character.is_lowercase() || matches!(character, ',' | ';' | ':'))
}

/// Whether the model carried straight on into a new sentence, leaving the full
/// stop to the rendering. A capital with no punctuation before it is the only
/// signal there is, and it is the one the recorded prose shows.
fn opens_new_sentence(after: &str) -> bool {
    after
        .trim_start()
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end().ends_with(['.', '!', '?'])
}

/// Joins a whole runtime sentence onto the model's paragraph.
///
/// The paragraph-scale case of the seam every marker substitution already
/// reads: the model routinely ends on a rendering that carries its own full
/// stop, and just as routinely ends on one that does not, so the runtime
/// closes the sentence rather than asking the model to.
pub(crate) fn append_sentence(mut text: String, sentence: &str) -> String {
    text.truncate(text.trim_end().len());
    if !ends_sentence(&text) {
        text.push('.');
    }
    text.push(' ');
    text.push_str(sentence);
    text
}

fn starts_with_article(text: &str) -> bool {
    text.split_whitespace()
        .next()
        .is_some_and(|word| matches!(word, "a" | "an" | "the"))
}

/// Drops a model-written article left dangling before a rendering that carries
/// its own. Only the word directly against the marker is a candidate — an
/// article any further back belongs to the model's own noun.
fn strip_trailing_article(text: &mut String) {
    let trimmed = text.trim_end();
    let start = trimmed
        .rfind(char::is_whitespace)
        .map_or(0, |index| index + 1);
    if matches!(&trimmed[start..], "a" | "an" | "the" | "A" | "An" | "The") {
        text.truncate(start);
    }
}

/// Any evaluation-shaped token, percentage or probability the model wrote in
/// its own words.
///
/// Bare integers pass: "won a knight two moves later" is prose, not a figure,
/// and no evaluation, percentage or probability can be written without a sign,
/// a decimal point, a mate glyph or a percent sign. URLs are exempt because the
/// only admissible URL is an exact learning-resource literal, checked whole.
fn contains_bare_figure(text: &str) -> bool {
    text.split_whitespace()
        .filter(|token| !token.contains("://"))
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '+' | '-' | '#' | '%')
            })
        })
        .any(is_figure)
}

fn is_figure(token: &str) -> bool {
    // A mate score carries its own sign inside the glyph: `#3` and `#-2` are
    // both evaluations, and only the digit that eventually follows says so.
    let unglyphed = token.strip_prefix('#');
    let body = unglyphed.unwrap_or(token);
    let unsigned = body.strip_prefix(['+', '-']);
    let signed = (unglyphed.is_some() || unsigned.is_some())
        && unsigned
            .unwrap_or(body)
            .starts_with(|character: char| character.is_ascii_digit());
    let percentage = token.strip_suffix('%').is_some_and(|head| {
        head.chars().all(|c| c.is_ascii_digit() || c == '.') && !head.is_empty()
    });
    signed || percentage || is_decimal(token)
}

fn is_decimal(token: &str) -> bool {
    token.split_once('.').is_some_and(|(whole, fraction)| {
        !whole.is_empty()
            && !fraction.is_empty()
            && whole.chars().all(|c| c.is_ascii_digit())
            && fraction.chars().all(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocabulary() -> MarkerVocabulary {
        let mut vocabulary = MarkerVocabulary::default();
        vocabulary.require("betterMove", "Nxd4");
        vocabulary.require("bestEval", "+1.3, Slightly better for White");
        vocabulary.offer("playedPopularity", "the most common choice at your rating");
        vocabulary
    }

    #[test]
    fn substitution_renders_every_figure_the_player_reads() {
        let grounded = vocabulary()
            .ground("You had {betterMove} there, which keeps you at {bestEval}.")
            .unwrap();

        assert_eq!(
            grounded.text,
            "You had Nxd4 there, which keeps you at +1.3, Slightly better for White."
        );
        assert!(!grounded.authored.contains("1.3"));
        assert_eq!(grounded.markers, vec!["betterMove", "bestEval"]);
    }

    #[test]
    fn a_figure_written_in_the_models_own_words_is_rejected() {
        for figure in ["+0.9", "-1.2", "0.0", "#3", "#-2", "37%"] {
            let text = format!("{{betterMove}} keeps you at {{bestEval}}, not {figure}.");
            assert_eq!(
                vocabulary().ground(&text).err(),
                Some(MarkerViolation::BareFigure),
                "{figure} should not be expressible"
            );
        }
    }

    #[test]
    fn prose_numbers_that_cannot_carry_an_evaluation_survive() {
        let grounded = vocabulary()
            .ground("{betterMove} wins a knight in 2 moves and leaves you at {bestEval}.")
            .unwrap();

        assert!(grounded.text.contains("in 2 moves"));
    }

    #[test]
    fn unknown_repeated_and_missing_markers_all_fail() {
        assert_eq!(
            vocabulary()
                .ground("{betterMove} {bestEval} {inventedMarker}")
                .err(),
            Some(MarkerViolation::UnknownMarker)
        );
        assert_eq!(
            vocabulary()
                .ground("{betterMove} again {betterMove} at {bestEval}")
                .err(),
            Some(MarkerViolation::RepeatedMarker("betterMove"))
        );
        assert_eq!(
            vocabulary().ground("{betterMove} was there.").err(),
            Some(MarkerViolation::MissingRequiredMarker("bestEval"))
        );
    }

    #[test]
    fn naming_the_played_move_again_is_reference_rather_than_a_repeated_claim() {
        let mut vocabulary = MarkerVocabulary::default();
        vocabulary.require_literal("playedMove", "h3");
        vocabulary.require("bestEval", "+1.3, Slightly better for White");

        let grounded = vocabulary
            .ground("You might have played {playedMove} to hold the centre, but after {playedMove} the position sits at {bestEval}.")
            .unwrap();

        assert_eq!(
            grounded.text,
            "You might have played h3 to hold the centre, but after h3 the position sits at +1.3, Slightly better for White."
        );
        // The ledger counts claims, so the second naming adds nothing to it.
        assert_eq!(grounded.markers, vec!["playedMove", "bestEval"]);
    }

    #[test]
    fn a_runtime_sentence_closes_the_models_last_one_only_when_the_model_left_it_open() {
        assert_eq!(
            append_sentence("You played h3. ".to_string(), "My best guess is e4."),
            "You played h3. My best guess is e4."
        );
        assert_eq!(
            append_sentence("You played h3".to_string(), "My best guess is e4."),
            "You played h3. My best guess is e4."
        );
    }

    #[test]
    fn a_stray_brace_is_an_unknown_marker_rather_than_surviving_substitution() {
        assert_eq!(
            vocabulary().ground("{betterMove} at {bestEval} }").err(),
            Some(MarkerViolation::UnknownMarker)
        );
        assert_eq!(
            vocabulary().ground("{betterMove} at {bestEval {").err(),
            Some(MarkerViolation::UnknownMarker)
        );
    }

    #[test]
    fn a_models_article_is_absorbed_by_a_rendering_that_carries_its_own() {
        let mut vocabulary = MarkerVocabulary::default();
        vocabulary.require("betterMove", "Nxd4");
        vocabulary.offer("evaluationLoss", "a 0.2-pawn margin");

        let grounded = vocabulary
            .ground("{betterMove} is close, with an {evaluationLoss} between them.")
            .unwrap();
        assert_eq!(
            grounded.text,
            "Nxd4 is close, with a 0.2-pawn margin between them."
        );

        // The model's article survives in the authored record: it is a word the
        // model wrote, and the authored form judges the model's claims.
        assert!(grounded.authored.contains("with an"));

        let grounded = vocabulary
            .ground("The {evaluationLoss} separates it from {betterMove}.")
            .unwrap();
        assert_eq!(
            grounded.text, "A 0.2-pawn margin separates it from Nxd4.",
            "an article that opened the sentence hands its position to the rendering"
        );
    }

    #[test]
    fn a_models_article_before_notation_is_left_alone() {
        let grounded = vocabulary()
            .ground("Meeting the {betterMove} keeps you at {bestEval}.")
            .unwrap();
        assert_eq!(
            grounded.text,
            "Meeting the Nxd4 keeps you at +1.3, Slightly better for White."
        );
    }

    #[test]
    fn an_article_belonging_to_the_models_own_noun_is_kept() {
        let mut vocabulary = MarkerVocabulary::default();
        vocabulary.require("betterMove", "Nxd4");
        vocabulary.offer("playedPopularity", "the most common choice at your rating");

        let grounded = vocabulary
            .ground("{betterMove} is strong, and the knight capture is {playedPopularity}.")
            .unwrap();
        assert_eq!(
            grounded.text,
            "Nxd4 is strong, and the knight capture is the most common choice at your rating."
        );
    }

    #[test]
    fn an_optional_marker_the_facts_do_not_carry_is_unknown() {
        let mut vocabulary = MarkerVocabulary::default();
        vocabulary.require("betterMove", "Nxd4");
        vocabulary.offer_available("playedPopularity", None);

        assert_eq!(
            vocabulary.ground("{betterMove}, {playedPopularity}.").err(),
            Some(MarkerViolation::UnknownMarker)
        );
    }
}
