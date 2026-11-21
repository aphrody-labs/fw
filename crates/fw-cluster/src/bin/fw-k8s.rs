//! `fw-k8s`: Kubernetes for fw (run by `fw cluster k8s`). JSON on stdout.
//!
//! fw-k8s status                     server version, nodes, GPUs, pod phases
//! fw-k8s nodes                      node facts (readiness, roles, CPU, memory, GPUs)
//! fw-k8s pods [-n NAMESPACE]        pods (all namespaces by default)
//! fw-k8s apply -f FILE|- [-n NS]    server-side apply of a YAML manifest (field manager `fw`)
//! fw-k8s topology [PROMPT]          the cluster's nodes as router nodes, and a routing decision

use fw_cluster::k8s;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str =
    "usage: fw-k8s <status|nodes|pods [-n NS]|apply -f FILE|- [-n NS]|topology [PROMPT]>";

async fn run(args: &[String]) -> Result<serde_json::Value, String> {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let cmd = args.first().map(String::as_str).ok_or(USAGE)?;
    if matches!(cmd, "-h" | "--help" | "help") {
        return Err(USAGE.into());
    }
    let client = kube::Client::try_default()
        .await
        .map_err(|e| format!("kubeconfig: {e}"))?;
    let e = |e: k8s::K8sError| e.to_string();
    Ok(match cmd {
        "status" => k8s::status(&client).await.map_err(e)?,
        "nodes" => serde_json::json!({ "nodes": k8s::nodes(&client).await.map_err(e)? }),
        "pods" => serde_json::json!({ "pods": k8s::pods(&client, flag("-n")).await.map_err(e)? }),
        "apply" => {
            let file = flag("-f").ok_or("apply needs -f FILE (or - for stdin)")?;
            let yaml = if file == "-" {
                let mut s = String::new();
                std::io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| e.to_string())?;
                s
            } else {
                std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?
            };
            serde_json::json!({ "applied": k8s::apply(&client, &yaml, flag("-n").unwrap_or("default")).await.map_err(e)? })
        }
        "topology" => {
            let mut router = k8s::router(&client, "k8s").await.map_err(e)?;
            let prompt = args.get(1).map_or("hello", String::as_str);
            let decision = router
                .nodes
                .iter()
                .any(|n| n.healthy)
                .then(|| router.route_request(prompt, false));
            serde_json::json!({ "nodes": router.nodes, "decision": decision })
        }
        other => return Err(format!("unknown command `{other}`\n{USAGE}")),
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fw-k8s: {e}");
            return ExitCode::from(1);
        }
    };
    match rt.block_on(run(&args)) {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fw-k8s: {e}");
            ExitCode::from(2)
        }
    }
}
