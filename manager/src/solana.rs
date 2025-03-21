use anyhow::{anyhow, Result};
use log::info;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Represents the Solana blockchain integration for the CryptoCrawl
pub struct SolanaIntegration {
    client: RpcClient,
    wallet: Keypair,
    program_id: Pubkey,
}

impl SolanaIntegration {
    /// Creates a new instance of SolanaIntegration
    pub fn new(url: &str, keypair_path: Option<&str>, program_id: &str) -> Result<Self> {
        // Connect to Solana devnet/testnet/mainnet
        let client = RpcClient::new_with_commitment(url.to_string(), CommitmentConfig::confirmed());
        
        // Load or generate wallet keypair
        let wallet = match keypair_path {
            Some(path) => Self::load_keypair(path)?,
            None => Keypair::new(),
        };

        // Convert program ID from string to Pubkey
        let program_id = Pubkey::from_str(program_id)
            .map_err(|e| anyhow!("Invalid program ID: {}", e))?;

        Ok(SolanaIntegration {
            client,
            wallet,
            program_id,
        })
    }

    /// Returns the public key (wallet address) as a string
    pub fn get_wallet_address(&self) -> String {
        self.wallet.pubkey().to_string()
    }

    /// Gets the current balance of the wallet
    pub fn get_balance(&self) -> Result<u64> {
        let balance = self.client.get_balance(&self.wallet.pubkey())?;
        Ok(balance)
    }

    /// Loads a keypair from a file
    fn load_keypair(path: &str) -> Result<Keypair> {
        let path = Path::new(path);
        
        // Try to load from file
        match File::open(path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                if let Err(e) = file.read_to_end(&mut bytes) {
                    info!("Could not read keypair file: {}, generating a new one", e);
                    return Ok(Keypair::new());
                }
                
                match Keypair::from_bytes(&bytes) {
                    Ok(keypair) => Ok(keypair),
                    Err(e) => {
                        info!("Invalid keypair format: {}, generating a new one", e);
                        Ok(Keypair::new())
                    }
                }
            },
            Err(e) => {
                info!("Could not open keypair file at {}: {}, generating a new one", path.display(), e);
                Ok(Keypair::new())
            }
        }
    }

    /// Submit verification result to the blockchain
    pub fn submit_verification_result(&self, task_id: &str, client_id: &str, is_verified: bool, score: f64) -> Result<String> {
        info!("Submitting verification result for task {}: client={}, verified={}, score={}", 
              task_id, client_id, is_verified, score);
              
        // For this implementation, we'll simulate the transaction
        // In a real implementation, we would create a proper transaction
        
        // Check if the blockchain is available
        let version = self.client.get_version()?;
        info!("Connected to Solana {} (feature set: {:?})", version.solana_core, version.feature_set);
        
        // Simulate a transaction hash
        let simulated_tx_hash = format!("verify_tx_{}_{}_{}_{}", 
                                         task_id, 
                                         client_id.replace(":", "_"), 
                                         if is_verified { "verified" } else { "rejected" }, 
                                         (score * 100.0) as u32);
        
        info!("Simulated verification transaction hash: {}", simulated_tx_hash);
        Ok(simulated_tx_hash)
    }
    
    /// Transfer incentives to a client
    pub fn transfer_incentives(&self, client_id: &str, amount: u64) -> Result<String> {
        info!("Transferring {} tokens to client {}", amount, client_id);
        
        // For this implementation, we'll simulate the transaction
        // In a real implementation, we would transfer SOL to the client's wallet
        
        // Simulate a transaction hash
        let simulated_tx_hash = format!("incentive_tx_{}_{}",
                                         client_id.replace(":", "_"),
                                         amount);
        
        info!("Simulated incentive transaction hash: {}", simulated_tx_hash);
        Ok(simulated_tx_hash)
    }
    
    /// Airdrop SOL to the wallet (useful for testing on devnet)
    pub async fn request_airdrop(&self, amount: u64) -> Result<String> {
        info!("Requesting airdrop of {} SOL to {}", amount as f64 / 1_000_000_000.0, self.wallet.pubkey());
        
        let signature = self.client.request_airdrop(&self.wallet.pubkey(), amount)?;
        self.client.confirm_transaction(&signature)?;
        
        info!("Airdrop successful: {}", signature);
        Ok(signature.to_string())
    }
} 