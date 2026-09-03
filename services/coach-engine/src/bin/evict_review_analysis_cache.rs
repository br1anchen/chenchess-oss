use chen_chess_coach_engine::review_analysis_cache::{
    evict_review_analysis_cache_from_env, ReviewAnalysisEvictionMode,
};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(about = "Evict expired review-analysis cache entries")]
struct Arguments {
    #[arg(long, value_enum)]
    environment: Environment,
    #[arg(long)]
    apply: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Environment {
    Staging,
    Production,
}

impl Environment {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let mode = if arguments.apply {
        ReviewAnalysisEvictionMode::Apply
    } else {
        ReviewAnalysisEvictionMode::DryRun
    };
    let report = evict_review_analysis_cache_from_env(arguments.environment.as_str(), mode).await?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
