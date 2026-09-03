//! Removing one manually imported Game from a Player's own records.
//!
//! The address names one Game Import; the delete spans the Game. The same Game
//! reviewed from the same side at three Elo Profiles is three Game Imports and
//! one Imported Game card, so a delete driven off the card alone would take one
//! record and orphan the other two. The other reviewed side is a different
//! review with different findings and is left standing.

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    digested_games::DigestedGameLookupError,
    game_import_store::{DeletedImportedGame, GameImportLookup, GameImportStoreError},
    imported_games::{imported_game_key, ImportedGameSourceIdentity},
    lichess::LichessExportClient,
    review_annotation_store::ReviewAnnotationAddress,
    review_session_contract::{
        CommandRejectionReason, GameImportId, ImportedGame, OperationCompletion, OperationKind,
        PlayerId, ProviderUnavailableReason, RejectionRecovery, RetryDirective,
    },
    review_share::revoke_grants_for_reviews,
    reviewed_games::ReviewedGameKey,
};

use super::{events::EventEmitter, ProcessorPrincipal, ReviewSessionProcessor};

/// One Game, named the way every listing surface names it, plus the address of
/// the Imported Game card that stands for it.
struct DeletedGameIdentity {
    key: ReviewedGameKey,
    imported_game_key: String,
}

impl DeletedGameIdentity {
    fn of(game: &ImportedGame) -> Option<Self> {
        let source = ImportedGameSourceIdentity::for_search(game)?;
        Some(Self {
            key: ReviewedGameKey {
                canonical_source_key: source.canonical_key(),
                review_side: game.review_side.into(),
            },
            imported_game_key: imported_game_key(&source, game.review_side),
        })
    }

    fn matches(&self, game: &ImportedGame) -> bool {
        Self::of(game).is_some_and(|other| other.key == self.key)
    }
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn delete_game_import(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: GameImportId,
        emitter: Arc<EventEmitter>,
    ) {
        let ProcessorPrincipal::Player(player_id) = principal else {
            emitter.rejected(
                OperationKind::GameImportDeletion,
                CommandRejectionReason::AuthenticationRequired,
                RejectionRecovery::None,
            );
            return;
        };
        let addressed = match self.game_imports.find(principal, &game_import_id).await {
            Ok(GameImportLookup::Found(record)) => *record,
            // A Game this Player does not own and a Game that never existed are
            // deliberately the same answer.
            Ok(GameImportLookup::NotFound | GameImportLookup::OwnerMismatch) => {
                emitter.rejected(
                    OperationKind::GameImportDeletion,
                    CommandRejectionReason::UnknownGameImport,
                    RejectionRecovery::None,
                );
                return;
            }
            Err(error) => return emit_persistence_failure(&emitter, &error),
        };
        let Some(identity) = DeletedGameIdentity::of(&addressed.imported_game) else {
            emitter.rejected(
                OperationKind::GameImportDeletion,
                CommandRejectionReason::UnknownGameImport,
                RejectionRecovery::None,
            );
            return;
        };
        let digested = match self.digested_games.digested_games(player_id).await {
            Ok(digested) => digested,
            Err(DigestedGameLookupError::Unavailable) => {
                emitter.unavailable(
                    OperationKind::GameImportDeletion,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return;
            }
        };
        if digested.contains(&identity.key) {
            emitter.rejected(
                OperationKind::GameImportDeletion,
                CommandRejectionReason::DigestedGameImport,
                RejectionRecovery::None,
            );
            return;
        }
        let records = match self.game_imports.list_game_import_records(principal).await {
            Ok(records) => records,
            Err(error) => return emit_persistence_failure(&emitter, &error),
        };
        let game_import_ids = records
            .iter()
            .filter(|record| identity.matches(&record.imported_game))
            .map(|record| record.game_import_id.clone())
            .chain(std::iter::once(game_import_id.clone()))
            .collect::<BTreeSet<_>>();
        if let Err(error) = self
            .game_imports
            .delete_imported_game(
                principal,
                DeletedImportedGame {
                    game_import_ids: game_import_ids.iter().cloned().collect(),
                    imported_game_key: identity.imported_game_key,
                },
            )
            .await
        {
            return emit_persistence_failure(&emitter, &error);
        }
        self.forget_deleted_reviews(principal, player_id, &game_import_ids)
            .await;
        emitter.completed(OperationCompletion::GameImportDeleted {
            game_import_id,
            /* The Player's whole shelf is capped well under this, so the
            saturation is arithmetic hygiene rather than a case. */
            deleted_import_count: u16::try_from(game_import_ids.len()).unwrap_or(u16::MAX),
        });
    }

    /// Everything that outlives the record: the resident session an open tab
    /// would keep answering from, the published comments a re-import at the
    /// same address would inherit, and the links that would resolve into it.
    ///
    /// The records are already gone by the time this runs, so a failure here
    /// costs residue rather than the delete, and is logged instead of answered
    /// with: telling the Player their delete failed when the Game is gone would
    /// be the wrong answer.
    async fn forget_deleted_reviews(
        &self,
        principal: &ProcessorPrincipal,
        player_id: &PlayerId,
        game_import_ids: &BTreeSet<GameImportId>,
    ) {
        let mut sessions = self.sessions.lock().await;
        for game_import_id in game_import_ids {
            sessions.remove(game_import_id);
        }
        drop(sessions);
        for game_import_id in game_import_ids {
            let address = ReviewAnnotationAddress {
                owner: principal.clone(),
                game_import_id: game_import_id.clone(),
            };
            if let Err(error) = self.annotations.delete(&address).await {
                tracing::error!(
                    category = error.diagnostic_category(),
                    %error,
                    "failed to delete the annotations of a deleted Game"
                );
            }
        }
        if let Err(error) =
            revoke_grants_for_reviews(self.review_shares.as_ref(), player_id, game_import_ids).await
        {
            tracing::error!(
                category = "review_share",
                %error,
                "failed to revoke the Review Share Grants of a deleted Game"
            );
        }
    }
}

fn emit_persistence_failure(emitter: &EventEmitter, error: &GameImportStoreError) {
    tracing::error!(
        firestore_operation = "game_import_delete",
        category = error.diagnostic_category(),
        error = %error,
        "Game Import persistence failed"
    );
    let retry = match error {
        GameImportStoreError::Configuration(_) | GameImportStoreError::InvalidRecord => {
            RetryDirective::NotRetryable
        }
        GameImportStoreError::Transport
        | GameImportStoreError::Unavailable
        | GameImportStoreError::Conflict => RetryDirective::RetryAllowed,
    };
    emitter.unavailable(
        OperationKind::GameImportDeletion,
        ProviderUnavailableReason::Persistence,
        retry,
    );
}
