use async_std::net::TcpListener;
use std::sync::Arc;
use structopt::StructOpt;
use surf::{Result, Url};
use tide::{log, Request, Response};

/// Client for fetching file tails
#[derive(Debug, StructOpt)]
struct Opt {
    /// Agent URL
    #[structopt(long, short)]
    url: Url,
    /// Key for accessing agent's API
    #[structopt(long, short = "k")]
    api_key: String,
    /// Output the last n lines, instead of the last 10
    #[structopt(long, short = "n")]
    lines: Option<u32>,
}

type State = Arc<Opt>;

static INDEX_HTML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/index.html"));

#[async_std::main]
async fn main() -> Result<()> {
    let opt = Opt::from_args();

    log::start();
    let mut app = tide::with_state(Arc::new(opt));

    let sse = |req: Request<State>| async move {
        let state = req.state();
        let mut url = state.url.join(&req.url().path()[4..])?;

        if let Some(n) = state.lines {
            url.query_pairs_mut().append_pair("n", &n.to_string());
        }

        let res: http_types::Response = surf::get(url)
            .header("X-Api-Key", state.api_key.clone())
            .await?
            .into();

        Ok(res)
    };

    app.at("/sse").get(sse);
    app.at("/sse/").get(sse);
    app.at("/sse/*").get(sse);

    let index = |_req| async {
        Ok(Response::builder(200)
            .header("Content-Type", "text/html")
            .body(INDEX_HTML))
    };

    app.at("/root").get(index);
    app.at("/root/").get(index);
    app.at("/root/*").get(index);

    let listener = TcpListener::bind("localhost:0").await?;

    let url = format!("http://localhost:{}/root", listener.local_addr()?.port());
    println!("{}", url);
    open::that_in_background(&url);

    app.listen(listener).await?;

    Ok(())
}
