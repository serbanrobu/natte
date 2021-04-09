use async_std::fs;
use async_std::prelude::*;
use serde::Deserialize;
use std::io::prelude::*;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use structopt::StructOpt;
use tide::utils::async_trait;
use tide::{log, sse, Middleware, Next, Request, Response, Result, StatusCode};

/// Agent for serving and tailing files
#[derive(Debug, StructOpt)]
struct Opt {
    /// IP of the server
    #[structopt(long, short, default_value = "127.0.0.1")]
    ip: IpAddr,
    /// Port of the server
    #[structopt(long, short, default_value = "0")]
    port: u16,
    /// API key
    #[structopt(long, short = "k")]
    api_key: String,
    /// The directory that will be served
    dir: PathBuf,
}

pub struct AuthMiddleware;

type State = Arc<Opt>;

#[async_trait]
impl Middleware<State> for AuthMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> Result {
        if !req
            .header("X-Api-Key")
            .map(|h| h.as_str() == req.state().api_key)
            .unwrap_or(false)
        {
            let mut res = Response::new(StatusCode::Unauthorized);
            res.set_body("invalid api key");
            return Ok(res);
        }

        Ok(next.run(req).await)
    }
}

#[derive(Deserialize)]
struct Query {
    n: Option<u32>,
}

#[async_std::main]
async fn main() -> Result<()> {
    log::start();

    let opt = Opt::from_args();
    let port = opt.port;
    let ip = opt.ip.clone();
    let mut app = tide::with_state(Arc::new(opt));
    app.with(AuthMiddleware);

    let sse = |req: Request<State>, sender: sse::Sender| async move {
        let query: Query = req.query()?;
        let path = PathBuf::from(&req.url().path()[1..]);
        let path = req.state().dir.join(path);

        if path.is_file() {
            let mut cmd = Command::new("tail");
            cmd.arg("-f")
                .arg(path.to_str().unwrap())
                .stdout(Stdio::piped());

            if let Some(n) = query.n {
                cmd.arg("-n").arg(n.to_string());
            }

            let child = cmd.spawn()?;
            let stdout = child.stdout.unwrap();
            let mut lines = BufReader::new(stdout).lines();

            while let Some(line) = lines.next().transpose()? {
                sender.send("", line, None).await?;
            }
        } else {
            let mut entries = fs::read_dir(path).await?;

            while let Some(entry) = entries.next().await.transpose()? {
                let name = if entry.metadata().await?.is_dir() {
                    "dir"
                } else {
                    "file"
                };

                sender
                    .send(name, entry.file_name().to_string_lossy(), None)
                    .await?;
            }
        }

        Ok(())
    };

    app.at("/").get(sse::endpoint(sse));
    app.at("/*").get(sse::endpoint(sse));

    app.listen(SocketAddr::new(ip, port)).await?;

    Ok(())
}
