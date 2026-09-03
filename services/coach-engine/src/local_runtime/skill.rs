use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use super::RuntimeError;

const COACH_SKILL_FILES: [(&str, &str); 4] = [
    (
        "SKILL.md",
        include_str!("../../../../skills/chenchess-coach/SKILL.md"),
    ),
    (
        "review-writing.md",
        include_str!("../../../../skills/chenchess-coach/review-writing.md"),
    ),
    (
        "manual-scenarios.md",
        include_str!("../../../../skills/chenchess-coach/manual-scenarios.md"),
    ),
    (
        "review-session.md",
        include_str!("../../../../skills/chenchess-coach/review-session.md"),
    ),
];

pub(super) fn install(target: &Path) -> Result<(), RuntimeError> {
    fs::create_dir_all(target)?;
    for (name, contents) in COACH_SKILL_FILES {
        fs::write(target.join(name), contents)?;
    }
    Ok(())
}

pub(super) fn sha256(target: &Path) -> Result<String, RuntimeError> {
    let mut digest = Sha256::new();
    for (name, _) in COACH_SKILL_FILES {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(fs::read(target.join(name))?);
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn verify(target: &Path, expected: &str) -> Result<(), RuntimeError> {
    let actual = sha256(target)?;
    if actual == expected {
        Ok(())
    } else {
        Err(RuntimeError::Checksum {
            label: "installed Coach Skill".to_string(),
            expected: expected.to_string(),
            actual,
        })
    }
}
