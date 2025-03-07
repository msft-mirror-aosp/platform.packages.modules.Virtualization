// Copyright 2024 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use api::debian_service_client::DebianServiceClient;
use api::ShutdownQueueOpeningRequest;
use std::process::Command;

use anyhow::anyhow;
use clap::Parser;
use log::debug;
pub mod api {
    tonic::include_proto!("com.android.virtualization.terminal.proto");
}

#[derive(Parser)]
/// Flags for running command
pub struct Args {
    /// grpc port number
    #[arg(long)]
    #[arg(alias = "grpc_port")]
    grpc_port: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let gateway_ip_addr = netdev::get_default_gateway()?.ipv4[0];

    let server_addr = format!("http://{}:{}", gateway_ip_addr.to_string(), args.grpc_port);

    debug!("connect to grpc server {}", server_addr);

    let mut client = DebianServiceClient::connect(server_addr).await.map_err(|e| e.to_string())?;

    let mut res_stream = client
        .open_shutdown_request_queue(tonic::Request::new(ShutdownQueueOpeningRequest {}))
        .await?
        .into_inner();

    while let Some(_response) = res_stream.message().await? {
        let status = Command::new("poweroff").status().expect("power off");
        if !status.success() {
            return Err(anyhow!("Failed to power off: {status}").into());
        }
        debug!("poweroff");
        break;
    }
    Ok(())
}
