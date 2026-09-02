//! `cargo run -p rusvel-evals -- --suite forge`
//!
//! Run all evals, or filter to a single suite. Exits non-zero on any failure
//! so CI can gate merges directly on this binary.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "rusvel-evals", about = "Run RUSVEL fixture-based evals")]
struct Args {
    /// Run only the named suite (e.g. forge, harvest, code, content, flow).
    #[arg(long)]
    suite: Option<String>,

    /// List registered evals and exit (no execution).
    #[arg(long)]
    list: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();

    if args.list {
        for eval in rusvel_evals::registry() {
            println!("{}\t{}", eval.suite(), eval.name());
        }
        return;
    }

    let report = rusvel_evals::run_suite(args.suite.as_deref()).await;
    print!("{}", report.render());
    if !report.all_passed() {
        std::process::exit(1);
    }
}
