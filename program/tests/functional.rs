#![cfg(feature = "test-bpf")]

use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_program_test::{processor, tokio, ProgramTest};
use solana_sdk::account::ReadableAccount;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;

#[tokio::test]
async fn test() {
    let program_test = ProgramTest::new(
        "multisig",
        multisig::id(),
        processor!(multisig::Processor::process),
    );

    // Start Program Test
    let (mut banks_client, funder, recent_blockhash) = program_test.start().await;

    // Create Multisig
    let seed = uuid::Uuid::new_v4().as_u128();

    let threshold = 2;

    let custodian_1 = Keypair::new();
    let custodian_2 = Keypair::new();
    let custodian_3 = Keypair::new();

    let mut transaction = Transaction::new_with_payer(
        &[multisig::create_multisig(
            &funder.pubkey(),
            seed,
            vec![
                custodian_1.pubkey(),
                custodian_2.pubkey(),
                custodian_3.pubkey(),
            ],
            threshold,
        )],
        Some(&funder.pubkey()),
    );
    transaction.sign(&[&funder], recent_blockhash);

    banks_client
        .process_transaction(transaction)
        .await
        .expect("process_transaction");

    let multisig_address = multisig::get_multisig_address(seed);

    let multisig_info = banks_client
        .get_account(multisig_address)
        .await
        .expect("get_account")
        .expect("account");

    let multisig_data = multisig::Multisig::unpack(multisig_info.data()).expect("multisig unpack");

    assert_eq!(multisig_data.is_initialized, true);
    assert_eq!(multisig_data.threshold, threshold);
    assert_eq!(
        multisig_data.owners,
        vec![
            custodian_1.pubkey(),
            custodian_2.pubkey(),
            custodian_3.pubkey()
        ]
    );

    // Add owners
    for _ in 0..7 {
        let seed = uuid::Uuid::new_v4().as_u128();

        let owner = Pubkey::new_unique();

        // Create Transaction instruction
        let mut transaction = Transaction::new_with_payer(
            &[multisig::create_transaction(
                &funder.pubkey(),
                &custodian_1.pubkey(),
                &multisig_address,
                seed,
                multisig::add_owner(&multisig_address, owner),
            )],
            Some(&funder.pubkey()),
        );
        transaction.sign(&[&funder, &custodian_1], recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .expect("process_transaction");

        let transaction_address = multisig::get_transaction_address(seed);

        let transaction_info = banks_client
            .get_account(transaction_address)
            .await
            .expect("get_account")
            .expect("account");

        let transaction_data = multisig::Transaction::unpack_from_slice(transaction_info.data())
            .expect("transaction unpack");

        assert_eq!(transaction_data.is_initialized, true);
        assert_eq!(transaction_data.did_execute, false);
        assert_eq!(transaction_data.program_id, multisig::id());

        // Approve
        let mut transaction = Transaction::new_with_payer(
            &[multisig::approve(
                &custodian_2.pubkey(),
                &multisig_address,
                &transaction_address,
            )],
            Some(&funder.pubkey()),
        );
        transaction.sign(&[&funder, &custodian_2], recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .expect("process_transaction");

        let transaction_info = banks_client
            .get_account(transaction_address)
            .await
            .expect("get_account")
            .expect("account");

        let transaction_data = multisig::Transaction::unpack_from_slice(transaction_info.data())
            .expect("transaction unpack");

        assert_eq!(transaction_data.signers[0], true);
        assert_eq!(transaction_data.signers[1], true);
        assert_eq!(transaction_data.signers[2], false);

        // Execute
        let accounts = transaction_data.accounts;

        let mut transaction = Transaction::new_with_payer(
            &[multisig::execute_transaction(
                &multisig_address,
                &transaction_address,
                accounts,
            )],
            Some(&funder.pubkey()),
        );

        transaction.sign(&[&funder], recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .expect("process_transaction");

        let transaction_info = banks_client
            .get_account(transaction_address)
            .await
            .expect("get_account")
            .expect("account");

        let transaction_data = multisig::Transaction::unpack_from_slice(transaction_info.data())
            .expect("transaction unpack");

        assert_eq!(transaction_data.did_execute, true);
    }

    // Check multisig owners
    let multisig_info = banks_client
        .get_account(multisig_address)
        .await
        .expect("get_account")
        .expect("account");

    let multisig_data = multisig::Multisig::unpack(multisig_info.data()).expect("multisig unpack");

    assert_eq!(multisig_data.owners.len(), multisig::MAX_SIGNERS);

    // Create pending transactions
    let mut pending_transactions = Vec::new();

    for _ in 0..multisig::MAX_TRANSACTIONS - 1 {
        let owner = Pubkey::new_unique();
        let seed = uuid::Uuid::new_v4().as_u128();

        // Create Transaction instruction
        let mut transaction = Transaction::new_with_payer(
            &[multisig::create_transaction(
                &funder.pubkey(),
                &custodian_1.pubkey(),
                &multisig_address,
                seed,
                multisig::add_owner(&multisig_address, owner),
            )],
            Some(&funder.pubkey()),
        );
        transaction.sign(&[&funder, &custodian_1], recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .expect("process_transaction");

        let transaction_address = multisig::get_transaction_address(seed);
        pending_transactions.push(transaction_address);
    }

    // Delete last pending transaction
    let seed = uuid::Uuid::new_v4().as_u128();
    let pending_transaction = *pending_transactions.last().unwrap();

    // Create Transaction instruction
    let mut transaction = Transaction::new_with_payer(
        &[multisig::create_transaction(
            &funder.pubkey(),
            &custodian_1.pubkey(),
            &multisig_address,
            seed,
            multisig::delete_pending_transaction(&multisig_address, pending_transaction),
        )],
        Some(&funder.pubkey()),
    );
    transaction.sign(&[&funder, &custodian_1], recent_blockhash);

    banks_client
        .process_transaction(transaction)
        .await
        .expect("process_transaction");

    // Check multisig pending transactions
    let multisig_info = banks_client
        .get_account(multisig_address)
        .await
        .expect("get_account")
        .expect("account");

    let multisig_data = multisig::Multisig::unpack(multisig_info.data()).expect("multisig unpack");

    assert_eq!(
        multisig_data.pending_transactions.len(),
        multisig::MAX_TRANSACTIONS
    );

    // Approve
    let transaction_address = multisig::get_transaction_address(seed);

    let mut transaction = Transaction::new_with_payer(
        &[multisig::approve(
            &custodian_2.pubkey(),
            &multisig_address,
            &transaction_address,
        )],
        Some(&funder.pubkey()),
    );
    transaction.sign(&[&funder, &custodian_2], recent_blockhash);

    banks_client
        .process_transaction(transaction)
        .await
        .expect("process_transaction");

    // Execute
    let transaction_info = banks_client
        .get_account(transaction_address)
        .await
        .expect("get_account")
        .expect("account");

    let transaction_data = multisig::Transaction::unpack_from_slice(transaction_info.data())
        .expect("transaction unpack");

    let accounts = transaction_data.accounts;

    let mut transaction = Transaction::new_with_payer(
        &[multisig::execute_transaction(
            &multisig_address,
            &transaction_address,
            accounts,
        )],
        Some(&funder.pubkey()),
    );

    transaction.sign(&[&funder], recent_blockhash);

    banks_client
        .process_transaction(transaction)
        .await
        .expect("process_transaction");

    // Check multisig pending transactions
    let multisig_info = banks_client
        .get_account(multisig_address)
        .await
        .expect("get_account")
        .expect("account");

    let multisig_data = multisig::Multisig::unpack(multisig_info.data()).expect("multisig unpack");

    assert_eq!(
        multisig_data.pending_transactions.len(),
        multisig::MAX_TRANSACTIONS - 2
    );
}
