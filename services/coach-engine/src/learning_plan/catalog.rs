use crate::{
    opening_identification::epd_from_fen,
    review_session_contract::{
        LearningResource, LearningResourceId, LearningResourceKind, LearningResourceMappingId,
        LearningResourceRole, LearningTrackKey, OpeningIdentificationProvenance, OpeningMetadata,
        OpeningServiceProvider,
    },
};

mod curriculum;

const NAJDORF_RESOURCE_MAPPING_ID: &str = "lichess:opening:sicilian-defense-najdorf-variation";
const VIENNA_ANDERSSEN_RESOURCE_MAPPING_ID: &str = "lichess:opening:vienna-game-anderssen-defense";
const CARO_KANN_CLASSICAL_RESOURCE_MAPPING_ID: &str =
    "lichess:opening:caro-kann-defense-classical-variation";

const NAJDORF_ECO: &str = "B90";
const NAJDORF_NAME: &str = "Sicilian Defense: Najdorf Variation";
const VIENNA_ANDERSSEN_EPD: &str = "rnbqk1nr/pppp1ppp/8/2b1p3/4P3/2N5/PPPP1PPP/R1BQKBNR w KQkq -";
const CARO_KANN_CLASSICAL_EPD: &str =
    "rn1q1rk1/p4ppp/1ppbpn2/4Nb2/3P1B2/5N2/PPP1BPPP/R2QK2R b KQ -";

struct ServiceMapping {
    provider: OpeningServiceProvider,
    eco: &'static str,
    name: &'static str,
    resource_mapping_id: &'static str,
}

const SERVICE_MAPPINGS: [ServiceMapping; 2] = [
    ServiceMapping {
        provider: OpeningServiceProvider::Lichess,
        eco: NAJDORF_ECO,
        name: NAJDORF_NAME,
        resource_mapping_id: NAJDORF_RESOURCE_MAPPING_ID,
    },
    ServiceMapping {
        provider: OpeningServiceProvider::ChessCom,
        eco: NAJDORF_ECO,
        name: NAJDORF_NAME,
        resource_mapping_id: NAJDORF_RESOURCE_MAPPING_ID,
    },
];

struct PositionMapping {
    epd: &'static str,
    resource_mapping_id: &'static str,
}

const POSITION_MAPPINGS: [PositionMapping; 2] = [
    PositionMapping {
        epd: VIENNA_ANDERSSEN_EPD,
        resource_mapping_id: VIENNA_ANDERSSEN_RESOURCE_MAPPING_ID,
    },
    PositionMapping {
        epd: CARO_KANN_CLASSICAL_EPD,
        resource_mapping_id: CARO_KANN_CLASSICAL_RESOURCE_MAPPING_ID,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ResourceCatalogError {
    #[error("the Learning Resource Catalog has no complete mapping")]
    MissingResourceMapping,
    #[error("the Learning Resource mapping identity is invalid")]
    InvalidMappingId,
    #[error("the supporting ply cannot be represented")]
    InvalidPly,
    #[error("the supporting ply does not exist in the imported Game")]
    MissingGameMove,
}

pub(crate) fn resources_for(
    key: &LearningTrackKey,
) -> Result<Vec<LearningResource>, ResourceCatalogError> {
    match key {
        LearningTrackKey::Curriculum { concept } => curriculum::resources_for(*concept),
        LearningTrackKey::Opening {
            resource_mapping_id,
        } if resource_mapping_id.as_str() == NAJDORF_RESOURCE_MAPPING_ID => opening_resources(
            "Sicilian_Defense_Najdorf_Variation",
            "Sicilian Defense: Najdorf",
        ),
        LearningTrackKey::Opening {
            resource_mapping_id,
        } if resource_mapping_id.as_str() == VIENNA_ANDERSSEN_RESOURCE_MAPPING_ID => {
            opening_resources(
                "Vienna_Game_Anderssen_Defense",
                "Vienna Game: Anderssen Defense",
            )
        }
        LearningTrackKey::Opening {
            resource_mapping_id,
        } if resource_mapping_id.as_str() == CARO_KANN_CLASSICAL_RESOURCE_MAPPING_ID => {
            opening_resources(
                "Caro-Kann_Defense_Classical_Variation",
                "Caro-Kann Defense: Classical Variation",
            )
        }
        LearningTrackKey::Opening { .. } => Err(ResourceCatalogError::MissingResourceMapping),
    }
}

pub(super) fn opening_key_for(
    opening_identification: &OpeningMetadata,
    fen: &str,
) -> Result<Option<(LearningTrackKey, LearningResourceMappingId)>, ResourceCatalogError> {
    let service_mapping = match opening_identification {
        OpeningMetadata::Present {
            eco,
            name,
            provenance: OpeningIdentificationProvenance::Service { provider, .. },
        } => SERVICE_MAPPINGS
            .iter()
            .find(|mapping| {
                mapping.provider == *provider && mapping.eco == eco && mapping.name == name
            })
            .map(|mapping| mapping.resource_mapping_id),
        OpeningMetadata::Present { .. } => None,
        OpeningMetadata::Absent => return Ok(None),
    };
    let mapping = service_mapping.or_else(|| {
        let epd = epd_from_fen(fen)?;
        POSITION_MAPPINGS
            .iter()
            .find(|mapping| mapping.epd == epd)
            .map(|mapping| mapping.resource_mapping_id)
    });
    mapping.map(opening_key).transpose()
}

fn opening_key(
    mapping: &'static str,
) -> Result<(LearningTrackKey, LearningResourceMappingId), ResourceCatalogError> {
    let resource_mapping_id = LearningResourceMappingId::try_from(mapping.to_string())
        .map_err(|_| ResourceCatalogError::InvalidMappingId)?;
    Ok((
        LearningTrackKey::Opening {
            resource_mapping_id: resource_mapping_id.clone(),
        },
        resource_mapping_id,
    ))
}

fn opening_resources(
    slug: &str,
    title: &str,
) -> Result<Vec<LearningResource>, ResourceCatalogError> {
    Ok(vec![
        resource(
            format!("lichess:opening-reference:{slug}"),
            LearningResourceRole::Learn,
            LearningResourceKind::OpeningReference,
            title.to_string(),
            format!("https://lichess.org/opening/{slug}"),
        )?,
        resource(
            format!("lichess:opening-puzzles:{slug}"),
            LearningResourceRole::Drill,
            LearningResourceKind::OpeningPuzzleStream,
            format!("{title} puzzles"),
            format!("https://lichess.org/training/{slug}"),
        )?,
    ])
}

fn resource(
    id: String,
    role: LearningResourceRole,
    kind: LearningResourceKind,
    title: String,
    canonical_url: String,
) -> Result<LearningResource, ResourceCatalogError> {
    Ok(LearningResource {
        resource_id: LearningResourceId::try_from(id)
            .map_err(|_| ResourceCatalogError::MissingResourceMapping)?,
        role,
        kind,
        title,
        canonical_url,
    })
}

#[cfg(test)]
mod tests {
    use crate::review_session_contract::{
        OpeningIdentificationProvenance, OpeningServiceAttribution, OpeningServiceProvider,
    };

    use super::*;

    #[test]
    fn opening_mapping_requires_an_exact_service_or_position_match() {
        let opening = OpeningMetadata::Present {
            eco: NAJDORF_ECO.to_string(),
            name: NAJDORF_NAME.to_string(),
            provenance: OpeningIdentificationProvenance::Service {
                provider: OpeningServiceProvider::Lichess,
                attribution: OpeningServiceAttribution::DirectImport,
            },
        };
        let (key, _) = opening_key_for(&opening, "8/8/8/8/8/8/8/8 w - - 0 1")
            .unwrap()
            .unwrap();
        assert_eq!(resources_for(&key).unwrap().len(), 2);
    }
}
