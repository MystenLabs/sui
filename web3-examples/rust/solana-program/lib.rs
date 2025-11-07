use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Define the type of state stored in accounts
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct CounterAccount {
    /// The counter value
    pub count: u64,
}

// Declare and export the program's entrypoint
entrypoint!(process_instruction);

/// Program entrypoint's implementation
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    msg!("Solana Counter Program Entrypoint");

    // Iterating accounts is safer than indexing
    let accounts_iter = &mut accounts.iter();

    // Get the account to store counter data
    let account = next_account_info(accounts_iter)?;

    // The account must be owned by the program in order to modify its data
    if account.owner != program_id {
        msg!("Counter account does not have the correct program id");
        return Err(ProgramError::IncorrectProgramId);
    }

    // Parse instruction data
    let instruction = instruction_data
        .get(0)
        .ok_or(ProgramError::InvalidInstructionData)?;

    match instruction {
        0 => {
            // Initialize
            msg!("Instruction: Initialize");
            let mut counter = CounterAccount::try_from_slice(&account.data.borrow())?;
            counter.count = 0;
            counter.serialize(&mut &mut account.data.borrow_mut()[..])?;
            msg!("Counter initialized to 0");
        }
        1 => {
            // Increment
            msg!("Instruction: Increment");
            let mut counter = CounterAccount::try_from_slice(&account.data.borrow())?;
            counter.count = counter.count.checked_add(1).unwrap();
            counter.serialize(&mut &mut account.data.borrow_mut()[..])?;
            msg!("Counter incremented to {}", counter.count);
        }
        2 => {
            // Decrement
            msg!("Instruction: Decrement");
            let mut counter = CounterAccount::try_from_slice(&account.data.borrow())?;
            counter.count = counter.count.checked_sub(1).unwrap();
            counter.serialize(&mut &mut account.data.borrow_mut()[..])?;
            msg!("Counter decremented to {}", counter.count);
        }
        _ => {
            msg!("Invalid instruction");
            return Err(ProgramError::InvalidInstructionData);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::clock::Epoch;

    #[test]
    fn test_counter() {
        let program_id = Pubkey::default();
        let key = Pubkey::default();
        let mut lamports = 0;
        let mut data = vec![0; 8];
        let owner = program_id;

        let account = AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            Epoch::default(),
        );

        let accounts = vec![account];

        // Initialize
        process_instruction(&program_id, &accounts, &[0]).unwrap();

        // Increment
        process_instruction(&program_id, &accounts, &[1]).unwrap();

        let counter = CounterAccount::try_from_slice(&accounts[0].data.borrow()).unwrap();
        assert_eq!(counter.count, 1);
    }
}
