# Web3 DApp Landing Page

Modern, responsive landing page for Web3 decentralized applications.

## Features

- ✅ Responsive design
- ✅ Web3 wallet connection (MetaMask)
- ✅ Smooth animations
- ✅ Modern gradient UI
- ✅ Intersection Observer animations
- ✅ Network detection
- ✅ Account change handling
- ✅ Mobile-friendly

## Preview

Modern landing page featuring:
- Hero section with CTAs
- Feature showcase
- Step-by-step guide
- Statistics dashboard
- Footer with links

## Technologies

- HTML5
- CSS3 (Flexbox, Grid)
- Vanilla JavaScript
- Web3.js (MetaMask integration)

## Setup

Simply open `index.html` in a browser:

```bash
# Local server (recommended)
python3 -m http.server 8000
# Visit http://localhost:8000

# Or use Live Server in VS Code
```

## Customization

### Colors

Edit CSS variables in `styles.css`:

```css
:root {
    --primary: #667eea;
    --secondary: #764ba2;
    --bg-dark: #0f0f23;
    --bg-card: #1a1a2e;
}
```

### Content

Modify text and sections in `index.html`:
- Hero title and subtitle
- Feature cards
- Statistics
- Footer links

### Wallet Connection

The Connect Wallet button uses MetaMask:

```javascript
const accounts = await window.ethereum.request({
    method: 'eth_requestAccounts'
});
```

## Features Breakdown

### Hero Section
- Large title with gradient text
- Call-to-action buttons
- Live statistics

### Features Grid
- 6 feature cards
- Hover animations
- Icon + description

### How It Works
- 3-step process
- Numbered steps
- Visual hierarchy

### Statistics
- Live stats display
- Animated counters
- Grid layout

### Footer
- Multi-column layout
- Social links
- Copyright info

## Responsive Design

Mobile breakpoints:
- Desktop: 1200px+
- Tablet: 768px - 1199px
- Mobile: < 768px

## Web3 Integration

### Connect Wallet

```javascript
// Check for Web3 provider
if (typeof window.ethereum !== 'undefined') {
    // MetaMask is installed
}

// Request connection
await window.ethereum.request({
    method: 'eth_requestAccounts'
});
```

### Listen for Events

```javascript
// Account changes
window.ethereum.on('accountsChanged', (accounts) => {
    // Handle account change
});

// Chain changes
window.ethereum.on('chainChanged', (chainId) => {
    // Handle network change
});
```

## Performance

- No external dependencies
- Optimized images (use WebP)
- Lazy loading
- CSS animations (GPU accelerated)

## SEO

- Semantic HTML5
- Meta tags for description
- Proper heading hierarchy
- Alt text for images

## Browser Support

- Chrome 90+
- Firefox 88+
- Safari 14+
- Edge 90+

## Deployment

### GitHub Pages

```bash
# Push to gh-pages branch
git subtree push --prefix web3-examples/html-css/dapp-landing origin gh-pages
```

### Vercel

```bash
vercel deploy
```

### Netlify

Drag and drop the folder to Netlify dashboard.

## Customization Guide

### Add New Section

```html
<section class="new-section">
    <div class="container">
        <h2 class="section-title">Your Title</h2>
        <!-- Your content -->
    </div>
</section>
```

### Add Feature Card

```html
<div class="feature-card">
    <div class="feature-icon">🎯</div>
    <h3>Feature Name</h3>
    <p>Feature description</p>
</div>
```

## Best Practices

1. **Optimize Images** - Use WebP format
2. **Minify CSS/JS** - For production
3. **Add Analytics** - Track user behavior
4. **SSL Certificate** - Use HTTPS
5. **Test Thoroughly** - Multiple devices/browsers

## Resources

- [MetaMask Docs](https://docs.metamask.io/)
- [Web3.js](https://web3js.readthedocs.io/)
- [CSS Grid](https://css-tricks.com/snippets/css/complete-guide-grid/)
- [Flexbox](https://css-tricks.com/snippets/css/a-guide-to-flexbox/)
