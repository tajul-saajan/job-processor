use actix_web::dev::ServerHandle;
use sqlx::{Pool, Postgres};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Handles graceful shutdown of the application
///
/// This module orchestrates graceful shutdown by:
/// 1. Listening for shutdown signals (SIGTERM, SIGINT/CTRL+C)
/// 2. Stopping the HTTP server (stops accepting new requests)
/// 3. Signaling workers to stop acquiring new jobs
/// 4. Waiting for workers to complete current jobs
/// 5. Closing database connections
pub struct ShutdownCoordinator {
    server_handle: ServerHandle,
    server_task: JoinHandle<Result<(), std::io::Error>>,
    worker_handles: Vec<JoinHandle<()>>,
    shutdown_tx: watch::Sender<bool>,
    pool: Pool<Postgres>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new(
        server_handle: ServerHandle,
        server_task: JoinHandle<Result<(), std::io::Error>>,
        worker_handles: Vec<JoinHandle<()>>,
        shutdown_tx: watch::Sender<bool>,
        pool: Pool<Postgres>,
    ) -> Self {
        Self {
            server_handle,
            server_task,
            worker_handles,
            shutdown_tx,
            pool,
        }
    }

    /// Wait for shutdown signal and perform graceful shutdown
    ///
    /// This function will block until either:
    /// - CTRL+C is received
    /// - SIGTERM is received (Unix only)
    ///
    /// Then it will:
    /// 1. Stop accepting new HTTP requests
    /// 2. Signal workers to stop
    /// 3. Wait for workers to finish current jobs
    /// 4. Close database connections
    pub async fn wait_for_shutdown(self) -> Result<(), std::io::Error> {
        // Setup signal handlers
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        // Wait for shutdown signal
        tokio::select! {
            _ = ctrl_c => {
                info!("Received CTRL+C signal, initiating graceful shutdown...");
            }
            _ = terminate => {
                info!("Received SIGTERM signal, initiating graceful shutdown...");
            }
        }

        // Perform graceful shutdown
        self.shutdown().await
    }

    /// Perform the actual shutdown sequence
    async fn shutdown(self) -> Result<(), std::io::Error> {
        // 1. Stop HTTP server (stop accepting new requests)
        info!("Stopping HTTP server (no longer accepting new requests)...");
        self.server_handle.stop(true).await;
        info!("HTTP server stopped accepting new requests");

        // 2. Signal workers to stop (they will finish current jobs)
        info!("Signaling workers to stop acquiring new jobs...");
        if let Err(e) = self.shutdown_tx.send(true) {
            error!("Failed to send shutdown signal to workers: {:?}", e);
        }

        // 3. Wait for all workers to finish current jobs
        let num_workers = self.worker_handles.len();
        info!("Waiting for {} workers to complete current jobs...", num_workers);
        let mut completed = 0;
        for (i, handle) in self.worker_handles.into_iter().enumerate() {
            match handle.await {
                Ok(_) => {
                    completed += 1;
                    info!("Worker {} stopped ({}/{})", i + 1, completed, num_workers);
                }
                Err(e) => error!("Worker {} failed to stop: {:?}", i + 1, e),
            }
        }
        info!("All workers stopped");

        // 4. Wait for HTTP server task to complete
        info!("Waiting for HTTP server to fully shut down...");
        match self.server_task.await {
            Ok(Ok(_)) => info!("HTTP server shut down successfully"),
            Ok(Err(e)) => error!("HTTP server encountered error during shutdown: {:?}", e),
            Err(e) => error!("HTTP server task panicked: {:?}", e),
        }

        // 5. Close database connections
        info!("Closing database connection pool...");
        self.pool.close().await;
        info!("Database connections closed");

        info!("Graceful shutdown completed successfully");
        Ok(())
    }
}
