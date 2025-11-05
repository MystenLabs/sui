import { JsonRpcProvider, devnetConnection, testnetConnection } from '@mysten/sui.js';

// Configuration
const NETWORK = 'devnet'; // Change to 'testnet' or 'mainnet' as needed
const PACKAGE_ID = 'YOUR_PACKAGE_ID'; // Replace with your deployed package ID
const MODULE_NAME = 'voting';

// Initialize provider
const provider = new JsonRpcProvider(NETWORK === 'devnet' ? devnetConnection : testnetConnection);

// Wallet state
let wallet = null;
let connectedAddress = null;

// Initialize app
document.addEventListener('DOMContentLoaded', () => {
    initializeTabs();
    initializeWallet();
    initializeForms();
});

// Tab navigation
function initializeTabs() {
    const tabButtons = document.querySelectorAll('.tab-btn');
    const tabContents = document.querySelectorAll('.tab-content');

    tabButtons.forEach(button => {
        button.addEventListener('click', () => {
            const tabName = button.getAttribute('data-tab');

            tabButtons.forEach(btn => btn.classList.remove('active'));
            tabContents.forEach(content => content.classList.remove('active'));

            button.classList.add('active');
            document.getElementById(`${tabName}-tab`).classList.add('active');
        });
    });
}

// Wallet connection
function initializeWallet() {
    const connectBtn = document.getElementById('connect-wallet');
    const disconnectBtn = document.getElementById('disconnect-wallet');

    connectBtn.addEventListener('click', connectWallet);
    disconnectBtn.addEventListener('click', disconnectWallet);

    // Check if wallet is already connected
    checkWalletConnection();
}

async function connectWallet() {
    try {
        // Check if Sui Wallet is installed
        if (!window.suiWallet) {
            alert('Please install Sui Wallet extension!');
            return;
        }

        // Request connection
        const accounts = await window.suiWallet.requestPermissions();

        if (accounts && accounts.length > 0) {
            connectedAddress = accounts[0];
            wallet = window.suiWallet;
            updateWalletUI(true);
        }
    } catch (error) {
        console.error('Failed to connect wallet:', error);
        showMessage('create-result', 'Failed to connect wallet: ' + error.message, 'error');
    }
}

function disconnectWallet() {
    wallet = null;
    connectedAddress = null;
    updateWalletUI(false);
}

function updateWalletUI(connected) {
    const walletSection = document.getElementById('wallet-section');
    const connectBtn = document.getElementById('connect-wallet');
    const walletInfo = document.getElementById('wallet-info');
    const addressSpan = document.getElementById('wallet-address');

    if (connected) {
        connectBtn.style.display = 'none';
        walletInfo.style.display = 'block';
        addressSpan.textContent = connectedAddress ?
            `${connectedAddress.slice(0, 6)}...${connectedAddress.slice(-4)}` : '';
    } else {
        connectBtn.style.display = 'block';
        walletInfo.style.display = 'none';
    }
}

function checkWalletConnection() {
    if (window.suiWallet && window.suiWallet.hasPermissions()) {
        window.suiWallet.getAccounts().then(accounts => {
            if (accounts && accounts.length > 0) {
                connectedAddress = accounts[0];
                wallet = window.suiWallet;
                updateWalletUI(true);
            }
        });
    }
}

// Forms initialization
function initializeForms() {
    // Create poll form
    document.getElementById('create-poll-form').addEventListener('submit', handleCreatePoll);
    document.getElementById('add-option').addEventListener('click', addOption);

    // Vote form
    document.getElementById('load-poll').addEventListener('click', loadPollForVoting);
    document.getElementById('vote-form').addEventListener('submit', handleVote);

    // Results
    document.getElementById('load-results').addEventListener('click', loadResults);
    document.getElementById('close-poll').addEventListener('click', closePoll);
    document.getElementById('reopen-poll').addEventListener('click', reopenPoll);
}

// Add extra option field
let optionCount = 2;
function addOption() {
    optionCount++;
    const extraOptions = document.getElementById('extra-options');
    const input = document.createElement('input');
    input.type = 'text';
    input.id = `option${optionCount}`;
    input.placeholder = `Option ${optionCount}`;
    extraOptions.appendChild(input);
}

// Create poll
async function handleCreatePoll(e) {
    e.preventDefault();

    if (!checkWalletConnected()) return;

    try {
        const question = document.getElementById('question').value;
        const options = [];

        for (let i = 1; i <= optionCount; i++) {
            const optionInput = document.getElementById(`option${i}`);
            if (optionInput && optionInput.value) {
                options.push(optionInput.value);
            }
        }

        if (options.length < 2) {
            showMessage('create-result', 'Please provide at least 2 options', 'error');
            return;
        }

        showMessage('create-result', 'Creating poll...', 'success');

        // Build transaction
        const tx = {
            packageObjectId: PACKAGE_ID,
            module: MODULE_NAME,
            function: options.length === 2 ? 'create_poll' : 'create_poll_multi',
            typeArguments: [],
            arguments: options.length === 2 ?
                [question, options[0], options[1]] :
                [question, options],
            gasBudget: 10000,
        };

        const result = await wallet.executeMoveCall(tx);

        showMessage('create-result',
            `Poll created successfully! Transaction: ${result.certificate.transactionDigest}`,
            'success');

        // Reset form
        document.getElementById('create-poll-form').reset();
        document.getElementById('extra-options').innerHTML = '';
        optionCount = 2;

    } catch (error) {
        console.error('Failed to create poll:', error);
        showMessage('create-result', 'Failed to create poll: ' + error.message, 'error');
    }
}

// Load poll for voting
async function loadPollForVoting() {
    const pollId = document.getElementById('poll-id').value.trim();

    if (!pollId) {
        showMessage('vote-result', 'Please enter a poll ID', 'error');
        return;
    }

    try {
        showMessage('vote-result', 'Loading poll...', 'success');

        const object = await provider.getObject(pollId);

        if (!object || !object.details) {
            showMessage('vote-result', 'Poll not found', 'error');
            return;
        }

        const fields = object.details.data.fields;
        const question = fields.question;
        const options = fields.options;
        const isActive = fields.is_active;

        if (!isActive) {
            showMessage('vote-result', 'This poll is closed', 'error');
            return;
        }

        // Display poll
        document.getElementById('poll-question').textContent = question;

        const voteOptionsContainer = document.getElementById('vote-options');
        voteOptionsContainer.innerHTML = '';

        options.forEach((option, index) => {
            const label = document.createElement('label');
            label.className = 'vote-option';
            label.innerHTML = `
                <input type="radio" name="vote-option" value="${index}" required>
                ${option}
            `;
            voteOptionsContainer.appendChild(label);
        });

        document.getElementById('poll-details').style.display = 'block';
        showMessage('vote-result', '', '');

    } catch (error) {
        console.error('Failed to load poll:', error);
        showMessage('vote-result', 'Failed to load poll: ' + error.message, 'error');
    }
}

// Cast vote
async function handleVote(e) {
    e.preventDefault();

    if (!checkWalletConnected()) return;

    const pollId = document.getElementById('poll-id').value.trim();
    const selectedOption = document.querySelector('input[name="vote-option"]:checked');

    if (!selectedOption) {
        showMessage('vote-result', 'Please select an option', 'error');
        return;
    }

    try {
        showMessage('vote-result', 'Casting vote...', 'success');

        const tx = {
            packageObjectId: PACKAGE_ID,
            module: MODULE_NAME,
            function: 'vote',
            typeArguments: [],
            arguments: [pollId, parseInt(selectedOption.value)],
            gasBudget: 10000,
        };

        const result = await wallet.executeMoveCall(tx);

        showMessage('vote-result',
            `Vote cast successfully! Transaction: ${result.certificate.transactionDigest}`,
            'success');

        // Reset form
        document.getElementById('vote-form').reset();

    } catch (error) {
        console.error('Failed to vote:', error);
        showMessage('vote-result', 'Failed to cast vote: ' + error.message, 'error');
    }
}

// Load results
async function loadResults() {
    const pollId = document.getElementById('results-poll-id').value.trim();

    if (!pollId) {
        showMessage('results-result', 'Please enter a poll ID', 'error');
        return;
    }

    try {
        showMessage('results-result', 'Loading results...', 'success');

        const object = await provider.getObject(pollId);

        if (!object || !object.details) {
            showMessage('results-result', 'Poll not found', 'error');
            return;
        }

        const fields = object.details.data.fields;
        const question = fields.question;
        const options = fields.options;
        const votes = fields.votes;
        const totalVotes = fields.total_votes;
        const isActive = fields.is_active;
        const creator = fields.creator;

        // Display results
        document.getElementById('results-question').textContent = question;
        document.getElementById('total-votes').textContent = totalVotes;

        const chartContainer = document.getElementById('results-chart');
        chartContainer.innerHTML = '';

        options.forEach((option, index) => {
            const voteCount = votes[index];
            const percentage = totalVotes > 0 ? (voteCount / totalVotes * 100).toFixed(1) : 0;

            const resultBar = document.createElement('div');
            resultBar.className = 'result-bar';
            resultBar.innerHTML = `
                <div class="result-option">${option}</div>
                <div class="bar-container">
                    <div class="bar-fill" style="width: ${percentage}%">
                        <span class="bar-text">${voteCount} votes (${percentage}%)</span>
                    </div>
                </div>
            `;
            chartContainer.appendChild(resultBar);
        });

        // Show poll actions if user is creator
        if (connectedAddress && connectedAddress === creator) {
            const pollActions = document.getElementById('poll-actions');
            pollActions.style.display = 'block';

            if (isActive) {
                document.getElementById('close-poll').style.display = 'inline-block';
                document.getElementById('reopen-poll').style.display = 'none';
            } else {
                document.getElementById('close-poll').style.display = 'none';
                document.getElementById('reopen-poll').style.display = 'inline-block';
            }
        }

        document.getElementById('results-display').style.display = 'block';
        showMessage('results-result', '', '');

    } catch (error) {
        console.error('Failed to load results:', error);
        showMessage('results-result', 'Failed to load results: ' + error.message, 'error');
    }
}

// Close poll
async function closePoll() {
    if (!checkWalletConnected()) return;

    const pollId = document.getElementById('results-poll-id').value.trim();

    try {
        const tx = {
            packageObjectId: PACKAGE_ID,
            module: MODULE_NAME,
            function: 'close_poll',
            typeArguments: [],
            arguments: [pollId],
            gasBudget: 10000,
        };

        await wallet.executeMoveCall(tx);
        showMessage('results-result', 'Poll closed successfully!', 'success');

        // Reload results
        setTimeout(() => loadResults(), 1000);

    } catch (error) {
        console.error('Failed to close poll:', error);
        showMessage('results-result', 'Failed to close poll: ' + error.message, 'error');
    }
}

// Reopen poll
async function reopenPoll() {
    if (!checkWalletConnected()) return;

    const pollId = document.getElementById('results-poll-id').value.trim();

    try {
        const tx = {
            packageObjectId: PACKAGE_ID,
            module: MODULE_NAME,
            function: 'reopen_poll',
            typeArguments: [],
            arguments: [pollId],
            gasBudget: 10000,
        };

        await wallet.executeMoveCall(tx);
        showMessage('results-result', 'Poll reopened successfully!', 'success');

        // Reload results
        setTimeout(() => loadResults(), 1000);

    } catch (error) {
        console.error('Failed to reopen poll:', error);
        showMessage('results-result', 'Failed to reopen poll: ' + error.message, 'error');
    }
}

// Utility functions
function checkWalletConnected() {
    if (!wallet || !connectedAddress) {
        alert('Please connect your wallet first!');
        return false;
    }
    return true;
}

function showMessage(elementId, message, type) {
    const element = document.getElementById(elementId);
    element.textContent = message;
    element.className = 'result-message';

    if (type) {
        element.classList.add(type);
    }
}
