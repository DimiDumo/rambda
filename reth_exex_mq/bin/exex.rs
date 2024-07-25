use chrono::Utc;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_exex_mq::proto::{
    remote_ex_ex_server::{RemoteExEx, RemoteExExServer},
    ExExNotification as ProtoExExNotification, SubscribeRequest as ProtoSubscribeRequest,
};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json;
use serde_json::Error as JsonError;
use std::any::type_name;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

struct ExExNotificationWrapper<'a>(&'a ExExNotification);

impl<'a> Serialize for ExExNotificationWrapper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let notification = self.0;

        if let Some(chain_committed) = notification.committed_chain() {
            let mut map = serializer.serialize_map(Some(1))?;

            let mut chain_committed_map = BTreeMap::new();
            let mut new_chain_map = BTreeMap::new();

            let blocks: BTreeMap<String, _> = chain_committed
                .blocks()
                .iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

            new_chain_map.insert("blocks", &blocks);
            chain_committed_map.insert("new", &new_chain_map);

            map.serialize_entry("ChainCommitted", &chain_committed_map)?;

            map.end()
        } else {
            // Handle other variants of ExExNotification
            // For now, we'll just serialize an empty map
            let map = serializer.serialize_map(Some(0))?;
            map.end()
        }
    }
}

#[derive(Debug)]
struct ExExService {
    notifications: broadcast::Sender<ExExNotification>,
}

#[tonic::async_trait]
impl RemoteExEx for ExExService {
    type SubscribeStream = ReceiverStream<Result<ProtoExExNotification, Status>>;

    async fn subscribe(
        &self,
        _request: Request<ProtoSubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let (tx, rx) = mpsc::channel(1);

        let mut notifications = self.notifications.subscribe();
        tokio::spawn(async move {
            while let Ok(notification) = notifications.recv().await {
                tx.send(Ok((&notification).try_into().expect("failed to encode")))
                    .await
                    .expect("failed to send notification to client");
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
    notifications: broadcast::Sender<ExExNotification>,
) -> eyre::Result<()> {
    while let Some(notification) = ctx.notifications.recv().await {
        if let Some(committed_chain) = notification.committed_chain() {
            println!("Sent committed chain event");
            ctx.events
                .send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
        }

        println!("Sent notificaion");
        let _ = notifications.send(notification.clone());

        // Try to serialize notification to JSON
        match serde_json::to_string(&ExExNotificationWrapper(&notification)) {
            Ok(json) => {
                // Create filename with current timestamp
                let timestamp = Utc::now().format("%Y%m%d%H%M%S%.3f").to_string();
                let filename = format!("/mnt/ssd/block_data/notification_{}.json", timestamp);

                // Write JSON to file
                if let Err(e) = write_to_file(&filename, &json) {
                    eprintln!("Failed to write notification to file: {}", e);
                } else {
                    println!("Wrote notification to file: {}", filename);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize notification: {}", e);
                log_detailed_error(&notification, &e);
            }
        }
    }

    Ok(())
}

fn write_to_file(filename: &str, content: &str) -> std::io::Result<()> {
    let path = Path::new(filename);
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn log_detailed_error(notification: &ExExNotification, error: &JsonError) {
    eprintln!("Detailed error information:");
    eprintln!("Error type: {}", type_name::<JsonError>());
    eprintln!("Error details: {:?}", error);

    // Log the type of notification
    eprintln!("Notification type: {}", type_name::<ExExNotification>());

    // Try to partially serialize the notification
    if let Ok(partial_json) = serde_json::to_value(notification) {
        if let Some(obj) = partial_json.as_object() {
            for (key, value) in obj {
                match serde_json::to_string(value) {
                    Ok(val_str) => {}
                    Err(_) => eprintln!(
                        "Field '{:?}' with Value {:?}: <failed to serialize>",
                        key, value
                    ),
                }
            }
        }
    } else {
        eprintln!("Failed to partially serialize notification");
    }

    // Print the error classification
    eprintln!("Error classification: {:?}", error.classify());

    // If it's an IO error, print more details
    if let Some(io_error) = error.io_error_kind() {
        eprintln!("IO error kind: {:?}", io_error);
    }

    // Add any custom debug information from the notification
    // This assumes you have implemented Debug for ExExNotification
    // eprintln!("Notification debug info: {:?}", notification);

    // Write raw debug output to file
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%.3f").to_string();
    let filename = format!(
        "/mnt/ssd/block_data_errors/notification_error_{}.txt",
        timestamp
    );
    let path = Path::new(&filename);

    if let Err(e) = std::fs::create_dir_all(path.parent().unwrap()) {
        eprintln!("Failed to create directory: {}", e);
        return;
    }

    match File::create(path) {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{:#?}", notification) {
                eprintln!("Failed to write to file: {}", e);
            } else {
                println!("Wrote error notification to file: {}", filename);
            }
        }
        Err(e) => eprintln!("Failed to create file: {}", e),
    }
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let notifications = broadcast::channel(1).0;

        let server = Server::builder()
            .add_service(RemoteExExServer::new(ExExService {
                notifications: notifications.clone(),
            }))
            .serve("[::]:10000".parse().unwrap());

        println!("PROTOBUF Server started");

        let handle = builder
            .node(EthereumNode::default())
            .install_exex("Remote", |ctx| async move { Ok(exex(ctx, notifications)) })
            .launch()
            .await?;

        handle
            .node
            .task_executor
            .spawn_critical("gRPC server", async move {
                server.await.expect("gRPC server crashed")
            });

        handle.wait_for_node_exit().await
    })
}
