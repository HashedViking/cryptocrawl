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
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        
        let keypair = Keypair::from_bytes(&bytes)
            .map_err(|e| anyhow!("Failed to load keypair: {}", e))?;
            
        Ok(keypair)
    }

    /// Submits crawl data to the blockchain (simplified for PoC)
    pub fn submit_crawl_data(&self, domain: &str, pages_count: usize, total_data_size: usize) -> Result<String> {
        info!("Submitting crawl data to Solana blockchain: {} pages from {}", pages_count, domain);
        
        // For this PoC, we'll just simulate sending a transaction with data
        // In a real implementation, this would call a program with the crawl data
        
        // Check if the blockchain is available
        let version = self.client.get_version()?;
        info!("Connected to Solana {} (feature set: {:?})", version.solana_core, version.feature_set);
        
        // In a real implementation, we would:
        // 1. Create a transaction with instruction to our program
        // 2. Sign and send the transaction
        // 3. Return the transaction signature
        
        // For the PoC, we'll just simulate a transaction hash
        let simulated_tx_hash = format!("sim_tx_{}_{}_{}", domain.replace(".", "_"), pages_count, total_data_size);
        
        info!("Simulated transaction hash: {}", simulated_tx_hash);
        Ok(simulated_tx_hash)
    }
    
    /// Claims incentives for completed crawl (simplified for PoC)
    pub fn claim_incentives(&self, tx_hash: &str) -> Result<u64> {
        info!("Claiming incentives for crawl transaction: {}", tx_hash);
        
        // In a real implementation, this would:
        // 1. Call the program to claim incentives
        // 2. Return the amount of tokens received
        
        // For this PoC, we'll simulate a calculation of incentives
        // based on the transaction hash content
        let simulated_incentive = tx_hash.len() as u64 * 1_000_000; // Example calculation
        
        info!("Simulated incentives claimed: {} tokens", simulated_incentive);
        Ok(simulated_incentive)
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