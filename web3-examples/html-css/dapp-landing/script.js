// Web3 DApp Landing Page JavaScript

// Wallet Connection
const connectWallet = document.getElementById('connectWallet');

if (connectWallet) {
    connectWallet.addEventListener('click', async () => {
        if (typeof window.ethereum !== 'undefined') {
            try {
                // Request account access
                const accounts = await window.ethereum.request({
                    method: 'eth_requestAccounts'
                });

                const account = accounts[0];
                const shortAddress = `${account.slice(0, 6)}...${account.slice(-4)}`;

                connectWallet.textContent = shortAddress;
                connectWallet.style.background = '#10b981';

                console.log('Connected:', account);

                // Get network
                const chainId = await window.ethereum.request({
                    method: 'eth_chainId'
                });
                console.log('Chain ID:', chainId);

            } catch (error) {
                console.error('Failed to connect wallet:', error);
                alert('Failed to connect wallet. Please try again.');
            }
        } else {
            alert('Please install MetaMask or another Web3 wallet!');
            window.open('https://metamask.io/', '_blank');
        }
    });
}

// Smooth Scrolling
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
        e.preventDefault();
        const target = document.querySelector(this.getAttribute('href'));
        if (target) {
            target.scrollIntoView({
                behavior: 'smooth',
                block: 'start'
            });
        }
    });
});

// Navbar scroll effect
let lastScroll = 0;
const navbar = document.querySelector('.navbar');

window.addEventListener('scroll', () => {
    const currentScroll = window.pageYOffset;

    if (currentScroll > 100) {
        navbar.style.background = 'rgba(15, 15, 35, 0.95)';
    } else {
        navbar.style.background = 'rgba(15, 15, 35, 0.8)';
    }

    lastScroll = currentScroll;
});

// Animated counters
function animateCounter(element, target, duration = 2000) {
    let start = 0;
    const increment = target / (duration / 16);
    const timer = setInterval(() => {
        start += increment;
        if (start >= target) {
            element.textContent = formatNumber(target);
            clearInterval(timer);
        } else {
            element.textContent = formatNumber(Math.floor(start));
        }
    }, 16);
}

function formatNumber(num) {
    if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
    if (num >= 1000) return (num / 1000).toFixed(0) + 'K';
    return num.toString();
}

// Intersection Observer for animations
const observerOptions = {
    threshold: 0.1,
    rootMargin: '0px 0px -100px 0px'
};

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.style.opacity = '1';
            entry.target.style.transform = 'translateY(0)';
        }
    });
}, observerOptions);

// Observe elements
document.querySelectorAll('.feature-card, .step, .stat-card').forEach(el => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(20px)';
    el.style.transition = 'all 0.6s ease-out';
    observer.observe(el);
});

// Check for Web3 provider on load
window.addEventListener('load', async () => {
    if (typeof window.ethereum !== 'undefined') {
        console.log('Web3 provider detected');

        // Listen for account changes
        window.ethereum.on('accountsChanged', (accounts) => {
            if (accounts.length === 0) {
                connectWallet.textContent = 'Connect Wallet';
                connectWallet.style.background = 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)';
            } else {
                const shortAddress = `${accounts[0].slice(0, 6)}...${accounts[0].slice(-4)}`;
                connectWallet.textContent = shortAddress;
            }
        });

        // Listen for chain changes
        window.ethereum.on('chainChanged', (chainId) => {
            console.log('Chain changed to:', chainId);
            window.location.reload();
        });
    }
});

// Copy address on click
connectWallet.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (window.ethereum && window.ethereum.selectedAddress) {
        navigator.clipboard.writeText(window.ethereum.selectedAddress);
        const originalText = connectWallet.textContent;
        connectWallet.textContent = 'Copied!';
        setTimeout(() => {
            const shortAddress = `${window.ethereum.selectedAddress.slice(0, 6)}...${window.ethereum.selectedAddress.slice(-4)}`;
            connectWallet.textContent = shortAddress;
        }, 1000);
    }
});
