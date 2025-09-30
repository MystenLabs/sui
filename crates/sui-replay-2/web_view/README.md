# Sui Transaction Web Viewer

A comprehensive web-based tool for analyzing Sui transaction replay data with an interactive tabbed interface. This viewer provides detailed analysis of transaction execution, gas usage, and object state changes.

## Features

### 📊 **Comprehensive Analysis Tabs**

#### **Overview Tab**
- Transaction details (digest, sender, epoch, checkpoint, status with color coding)
- Gas analysis table with proper number alignment
- Gas coins table showing object ID, version, and deletion status
- Transaction type information with detailed inputs and commands

#### **Objects Touched Tab**
- All objects and packages loaded during execution (may not appear in effects)
- Packages with version numbers and module counts
- Input vs dependency package categorization
- Comprehensive view including read-only access and dependencies

#### **Object Changes Tab**
- State changes captured in transaction effects
- Created, deleted, and modified objects with proper categorization
- Objects grouped by usage type (input, gas, runtime)

#### **Gas Analysis Tab**
- Detailed gas constants and cost breakdown
- Per-object gas usage tables with storage costs and rebates
- Created, deleted, and modified object gas analysis
- Gas validation and detailed cost attribution

### 🔗 **Explorer Integration**
- Configurable Sui explorer base URL (default: SuiVision)
- Clickable links for transactions, objects, accounts, and packages
- Visual link styling with hover effects

## How to Use

### 📁 **File Loading Options**

#### **Option 1: Browse Directory (Recommended)**
1. Open `index.html` in your web browser
2. Click "📁 Browse Directory" and select the replay directory
3. The app automatically detects these required JSON files:
   - `transaction_data.json`
   - `transaction_effects.json`
   - `transaction_gas_report.json`
   - `replay_cache_summary.json`
4. Status indicators show ✅ for found files and ❌ for missing files
5. Set your preferred explorer URL (defaults to SuiVision)
6. Click "🔍 Analyze Transaction" when ready
7. Navigate through the analysis tabs

#### **Option 2: Drag & Drop Directory**
1. Drag the entire replay directory into the designated drop area
2. Files are automatically scanned and loaded
3. Proceed with analysis

#### **Option 3: Manual Path Entry**
- Type or paste the replay directory path for reference
- Still requires Browse or Drag & Drop for actual file loading (browser security)

## File Structure

```
web_view/
├── index.html              # Main web application
├── styles.css              # Dark theme styling with responsive design
├── transaction-viewer.js   # Core analysis logic and UI generation
└── README.md              # This documentation
```

## Running the Web App

### **Simple Method (File Protocol)**
```bash
# Navigate to the web_view directory
cd /path/to/sui-replay-2/web_view

# Open directly in browser
open index.html
# or
firefox index.html
```

### **Local Web Server (Recommended)**
For better performance and to avoid potential CORS issues:

```bash
# Using Python (recommended)
cd /path/to/sui-replay-2/web_view
python3 -m http.server 8000
# Open http://localhost:8000

# Using Node.js
cd /path/to/sui-replay-2/web_view
npx serve
# Follow the provided URL

# Using any other static server
cd /path/to/sui-replay-2/web_view
# Use your preferred static file server
```

## Key Features & Improvements

### **Visual Enhancements**
- ✅ Color-coded transaction status (green for success, red for failure)
- 📊 Properly aligned gas cost tables with monospace fonts
- 🏷️ Version numbers displayed for all packages
- 🎨 Consistent visual hierarchy with overview sections
- 📱 Responsive design for various screen sizes

### **Gas Analysis**
- Detailed per-object storage costs and rebates
- Gas coin tracking with deletion status
- Comprehensive cost breakdown tables
- Gas validation and consistency checking

### **Object Analysis**
- Complete object lifecycle tracking (created/deleted/modified)
- Usage categorization (input, gas, runtime objects)
- Package dependency analysis with version information
- Explorer link integration for easy navigation

### **User Experience**
- Contextual help text for each analysis section
- Real-time file loading status
- Error handling with descriptive messages
- Intuitive navigation between analysis views

## Example Replay Directory

After running the sui-replay-2 tool, files are located at:
```
.replay/[TRANSACTION_DIGEST]/
├── transaction_data.json         # Transaction input and structure
├── transaction_effects.json      # State changes and execution results
├── transaction_gas_report.json   # Gas usage breakdown and costs
└── replay_cache_summary.json     # Cached objects and packages
```

## Technical Implementation

### **Core Architecture**
- Pure JavaScript with no external dependencies
- Modular class-based design for analysis logic
- Dynamic HTML generation with proper escaping
- Comprehensive error handling and validation

### **Analysis Pipeline**
1. **File Loading**: FileReader API for local file access
2. **Data Processing**: JSON parsing with error recovery
3. **Object Analysis**: Transaction effect processing and categorization
4. **Gas Analysis**: Cost calculation and per-object breakdown
5. **UI Generation**: Dynamic tab creation with styled content

### **Styling Philosophy**
- Dark terminal-inspired theme for technical data
- Monospace fonts for IDs, addresses, and numerical data
- Color coding for status, types, and interactive elements
- Responsive layout with proper spacing and hierarchy

## Integration with Sui Replay Tool

This web viewer is designed to work seamlessly with the sui-replay-2 tool:

```bash
# Generate replay data
/path/to/sui/target/debug/sui-replay-2 --digest YOUR_TX_DIGEST --overwrite-existing --store-mode inmem-fs-gql

# Analyze with web viewer
cd /path/to/sui-replay-2/web_view
python3 -m http.server 8000
# Navigate to http://localhost:8000 and load .replay/YOUR_TX_DIGEST/
```

The web viewer provides the same comprehensive analysis as the Python scripts but with enhanced usability and visual presentation.