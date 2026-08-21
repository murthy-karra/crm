#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let config = match crm_api::config::Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("invalid configuration: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = crm_api::run(config).await {
        eprintln!("server error: {err}");
        std::process::exit(1);
    }
}
