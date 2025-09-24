// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

class TransactionViewer {
    constructor() {
        this.files = {
            'transaction_data': null,
            'transaction_effects': null,
            'transaction_gas_report': null,
            'replay_cache_summary': null
        };

        this.requiredFiles = [
            'transaction_data.json',
            'transaction_effects.json',
            'transaction_gas_report.json',
            'replay_cache_summary.json'
        ];

        this.setupEventListeners();
    }

    // Explorer configuration mapping
    getExplorerConfig() {
        const explorers = {
            'suiscan': {
                mainnet: {
                    baseUrl: 'https://suiscan.xyz/mainnet',
                    paths: {
                        transaction: 'tx',        // Transaction: suiscan -> tx
                        address: 'account',       // Account: suiscan -> account
                        object: 'object',         // Object: suiscan -> object
                        package: 'object'         // Package: suiscan -> object
                    }
                },
                testnet: {
                    baseUrl: 'https://suiscan.xyz/testnet',
                    paths: {
                        transaction: 'tx',        // Transaction: suiscan -> tx
                        address: 'account',       // Account: suiscan -> account
                        object: 'object',         // Object: suiscan -> object
                        package: 'object'         // Package: suiscan -> object
                    }
                }
            },
            'suivision': {
                mainnet: {
                    baseUrl: 'https://suivision.xyz',
                    paths: {
                        transaction: 'txblock',   // Transaction: suivision -> txblock
                        address: 'account',       // Account: suivision -> account
                        object: 'object',         // Object: suivision -> object
                        package: 'package'        // Package: suivision -> package
                    }
                },
                testnet: {
                    baseUrl: 'https://testnet.suivision.xyz',
                    paths: {
                        transaction: 'txblock',   // Transaction: suivision -> txblock
                        address: 'account',       // Account: suivision -> account
                        object: 'object',         // Object: suivision -> object
                        package: 'package'        // Package: suivision -> package
                    }
                }
            }
        };

        const selectedExplorer = document.getElementById('explorer-select').value;
        return explorers[selectedExplorer] || explorers['suiscan'];
    }

    getCurrentNetwork() {
        // Try to get network from the loaded cache data
        if (this.cacheData && this.cacheData.network) {
            return this.cacheData.network;
        }
        // Default fallback (though this shouldn't happen with the new network field)
        return 'mainnet';
    }


    createExplorerLink(id, type, text = null) {
        const displayText = text || id;

        // Get network from cache data if available
        const network = this.getCurrentNetwork();

        // Only create links for mainnet and testnet
        if (network !== 'mainnet' && network !== 'testnet') {
            return this.encodeHTML(displayText);
        }

        const explorerConfig = this.getExplorerConfig();
        const config = explorerConfig[network];

        if (!config) {
            return this.encodeHTML(displayText);
        }

        let path;

        // Map our internal types to the explorer's path structure
        switch (type) {
            case 'txblock':
                path = config.paths.transaction;
                break;
            case 'account':
                path = config.paths.address;
                break;
            case 'object':
                path = config.paths.object;
                break;
            case 'package':
                path = config.paths.package;
                break;
            default:
                return this.encodeHTML(displayText);
        }

        // Validate and sanitize URL components
        const baseUrl = this.validateExplorerUrl(config.baseUrl);
        const safePath = this.sanitizePath(path);
        const safeId = this.sanitizeId(id);

        if (!baseUrl) {
            return this.encodeHTML(displayText);
        }

        const url = `${baseUrl}/${safePath}/${safeId}`;
        const safeDisplayText = this.encodeHTML(displayText);
        const safeUrl = this.encodeHTML(url);
        return `<a href="${safeUrl}" target="_blank" class="explorer-link">${safeDisplayText}</a>`;
    }

    /**
     * Validates explorer URL to prevent injection attacks.
     * Only allows HTTPS URLs with safe characters.
     */
    validateExplorerUrl(url) {
        try {
            const trimmed = url.trim();
            if (
                trimmed.startsWith('https://')
                && !/[<>"']/g.test(trimmed)
                && /^https:\/\/[A-Za-z0-9.-]+(:\d+)?(\/.*)?$/.test(trimmed)
            ) {
                return trimmed;
            }
        } catch (e) {
            // Invalid URL, fall through to default
        }
        return 'https://suiscan.xyz';
    }

    /**
     * HTML-encode text to prevent breaking out of tags and XSS attacks.
     */
    encodeHTML(s) {
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    /**
     * Validates explorer URLs to ensure they are safe HTTPS URLs
     */
    validateExplorerUrl(url) {
        try {
            const trimmed = String(url).trim();
            if (!trimmed.startsWith('https://')) {
                return null;
            }
            if (/[<>"']/g.test(trimmed)) {
                return null;
            }
            if (!/^https:\/\/[A-Za-z0-9.-]+(:\d+)?(\/.*)?$/.test(trimmed)) {
                return null;
            }
            return trimmed;
        } catch (e) {
            return null;
        }
    }

    /**
     * Encodes HTML entities to prevent XSS
     */
    encodeHTML(s) {
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    /**
     * Sanitizes ID parameters - allows only safe characters for blockchain identifiers.
     */
    sanitizeId(id) {
        // For blockchain IDs, allow only alphanumeric, hyphens, underscores and 'x' prefix
        return String(id).replace(/[^a-zA-Z0-9\-_x]/g, '');
    }

    /**
     * Sanitizes URL path components
     */
    sanitizePath(path) {
        // Allow only alphanumeric characters, hyphens, underscores, and slashes for paths
        return String(path).replace(/[^a-zA-Z0-9\-_\/]/g, '');
    }

    // Sortable table functionality
    makeSortable(tableId) {
        const table = document.getElementById(tableId);
        if (!table) return;

        const thead = table.querySelector('thead');
        if (!thead) return; // Skip tables without proper thead structure

        const headers = table.querySelectorAll('thead th');
        headers.forEach((header, index) => {
            header.style.cursor = 'pointer';
            header.style.userSelect = 'none';
            header.classList.add('sortable-header');

            // Add sort indicator container
            const originalText = header.innerHTML;
            header.innerHTML = `${originalText} <span class="sort-indicator"></span>`;

            header.addEventListener('click', () => {
                this.sortTable(table, index, header);
            });
        });
    }

    sortTable(table, columnIndex, header) {
        const tbody = table.querySelector('tbody');
        const rows = Array.from(tbody.querySelectorAll('tr'));

        // Determine current sort direction
        const isAscending = header.classList.contains('sort-asc');

        // Remove all sort classes from headers
        table.querySelectorAll('th').forEach(th => {
            th.classList.remove('sort-asc', 'sort-desc');
            const indicator = th.querySelector('.sort-indicator');
            if (indicator) indicator.textContent = '';
        });

        // Determine new sort direction
        const sortAscending = !isAscending;

        // Add appropriate class and indicator
        if (sortAscending) {
            header.classList.add('sort-asc');
            header.querySelector('.sort-indicator').textContent = ' ↑';
        } else {
            header.classList.add('sort-desc');
            header.querySelector('.sort-indicator').textContent = ' ↓';
        }

        // Sort rows
        rows.sort((a, b) => {
            const aCell = a.cells[columnIndex];
            const bCell = b.cells[columnIndex];

            if (!aCell || !bCell) return 0;

            // Get text content for comparison (strip HTML tags)
            const aText = aCell.textContent || aCell.innerText || '';
            const bText = bCell.textContent || bCell.innerText || '';

            // Try to parse as numbers
            const aNum = parseFloat(aText.replace(/[^0-9.-]/g, ''));
            const bNum = parseFloat(bText.replace(/[^0-9.-]/g, ''));

            let comparison = 0;

            // If both are numbers, sort numerically
            if (!isNaN(aNum) && !isNaN(bNum)) {
                comparison = aNum - bNum;
            } else {
                // Sort alphabetically
                comparison = aText.localeCompare(bText);
            }

            return sortAscending ? comparison : -comparison;
        });

        // Reorder rows in DOM
        rows.forEach(row => tbody.appendChild(row));
    }

    formatNumber(num) {
        // Format number with underscore thousands separator
        if (num === null || num === undefined || num === '') {
            return 'N/A';
        }

        const numStr = num.toString();
        // Only format if it's a valid number
        if (!/^\d+$/.test(numStr)) {
            return numStr;
        }

        return numStr.replace(/\B(?=(\d{3})+(?!\d))/g, '_');
    }

    setupCustomTooltips() {
        // Remove existing tooltip event listeners to avoid duplicates
        document.querySelectorAll('.custom-tooltip').forEach(tooltip => {
            tooltip.removeEventListener('mouseenter', this.showTooltip);
            tooltip.removeEventListener('mouseleave', this.scheduleHideTooltip);
        });

        // Add new tooltip event listeners
        const tooltips = document.querySelectorAll('.custom-tooltip');

        tooltips.forEach((tooltip) => {
            tooltip.addEventListener('mouseenter', this.showTooltip.bind(this));
            tooltip.addEventListener('mouseleave', this.scheduleHideTooltip.bind(this));
        });
    }

    showTooltip(event) {
        const element = event.target;
        const tooltipText = element.getAttribute('data-tooltip');

        // Create tooltip element
        let tooltip = document.getElementById('active-tooltip');
        if (!tooltip) {
            tooltip = document.createElement('div');
            tooltip.id = 'active-tooltip';
            tooltip.style.cssText = `
                position: fixed;
                background: linear-gradient(135deg, #2a2a2a, #1a1a1a);
                color: #ffffff;
                padding: 8px 12px;
                border-radius: 6px;
                border: 1px solid #4a9eff;
                font-size: 11px;
                z-index: 1000;
                pointer-events: auto;
                font-family: Monaco, Menlo, Consolas, monospace;
                box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
                max-width: 350px;
                word-break: break-word;
                opacity: 0;
                transition: opacity 0.2s ease-in-out;
                user-select: text;
                cursor: text;
            `;
            document.body.appendChild(tooltip);
        }

        tooltip.textContent = tooltipText;

        // Position tooltip
        const rect = element.getBoundingClientRect();
        const tooltipRect = tooltip.getBoundingClientRect();

        let left = rect.left;
        let top = rect.top - tooltipRect.height - 10;

        // Adjust if tooltip would go off the right edge
        if (left + tooltipRect.width > window.innerWidth - 10) {
            left = window.innerWidth - tooltipRect.width - 10;
        }

        // Adjust if tooltip would go off the left edge
        if (left < 10) {
            left = 10;
        }

        // Adjust if tooltip would go off the top
        if (top < 10) {
            top = rect.bottom + 10;
        }

        tooltip.style.left = left + 'px';
        tooltip.style.top = top + 'px';
        tooltip.style.opacity = '1';

        // Add tooltip hover listeners to keep it visible when hovering over it
        tooltip.addEventListener('mouseenter', this.cancelHideTooltip.bind(this));
        tooltip.addEventListener('mouseleave', this.scheduleHideTooltip.bind(this));

        // Clear any existing hide timeout
        this.cancelHideTooltip();
    }

    scheduleHideTooltip() {
        // Schedule tooltip to hide after a short delay
        this.tooltipHideTimeout = setTimeout(() => {
            this.hideTooltip();
        }, 300);
    }

    cancelHideTooltip() {
        // Cancel any scheduled tooltip hiding
        if (this.tooltipHideTimeout) {
            clearTimeout(this.tooltipHideTimeout);
            this.tooltipHideTimeout = null;
        }
    }

    hideTooltip() {
        const tooltip = document.getElementById('active-tooltip');
        if (tooltip) {
            tooltip.style.opacity = '0';
            setTimeout(() => {
                if (tooltip.parentNode) {
                    tooltip.parentNode.removeChild(tooltip);
                }
            }, 200);
        }
    }

    setupEventListeners() {
        // Directory input
        document.getElementById('directory-input').addEventListener('input', () => {
            // Note: Text input for path can't directly load files due to security restrictions
            // This is mainly for display purposes
        });

        // Directory browser button
        document.getElementById('browse-directory').addEventListener('click', () => {
            document.getElementById('directory-picker').click();
        });

        // Directory picker (webkitdirectory)
        document.getElementById('directory-picker').addEventListener('change', (e) => {
            this.handleDirectorySelect(e.target.files);
        });

        // Drag and drop
        const dropArea = document.getElementById('drag-drop-area');
        dropArea.addEventListener('dragover', this.handleDragOver.bind(this));
        dropArea.addEventListener('drop', this.handleDrop.bind(this));
        dropArea.addEventListener('dragenter', this.handleDragEnter.bind(this));
        dropArea.addEventListener('dragleave', this.handleDragLeave.bind(this));

        // Analyze button
        document.getElementById('analyze-btn').addEventListener('click', this.analyzeTransaction.bind(this));

        // Tab navigation
        this.setupTabNavigation();
    }

    setupTabNavigation() {
        // Add click listeners to existing tabs
        document.addEventListener('click', (e) => {
            if (e.target.classList.contains('tab-btn')) {
                const tabName = e.target.getAttribute('data-tab');
                this.switchToTab(tabName);
            }
        });
    }

    createAnalysisTabs() {
        const tabNav = document.getElementById('tab-nav');

        // Clear existing analysis tabs (keep only Load Files tab)
        const existingTabs = tabNav.querySelectorAll('.tab-btn');
        existingTabs.forEach(tab => {
            if (tab.getAttribute('data-tab') !== 'load') {
                tab.remove();
            }
        });

        // Create new tabs for analysis results
        const tabs = [
            { id: 'overview', label: 'Overview' },
            { id: 'objects', label: 'Objects Touched' },
            { id: 'changes', label: 'Object Changes' },
            { id: 'gas', label: 'Gas Analysis' }
        ];

        tabs.forEach(tab => {
            const tabBtn = document.createElement('button');
            tabBtn.className = 'tab-btn';
            tabBtn.setAttribute('data-tab', tab.id);
            tabBtn.textContent = tab.label;
            tabNav.appendChild(tabBtn);
        });
    }

    switchToTab(tabName) {
        // Update tab buttons
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.classList.remove('active');
        });

        const targetBtn = document.querySelector(`[data-tab="${tabName}"]`);
        if (targetBtn) {
            targetBtn.classList.add('active');
        }

        // Update tab panels
        document.querySelectorAll('.tab-panel').forEach(panel => {
            panel.classList.remove('active');
        });

        const targetPanel = document.getElementById(`tab-${tabName}`);
        if (targetPanel) {
            targetPanel.classList.add('active');
        }
    }

    handleDirectorySelect(fileList) {
        const files = Array.from(fileList);
        this.processDirectoryFiles(files);
    }

    processDirectoryFiles(files) {
        // Reset files
        this.files = {
            'transaction_data': null,
            'transaction_effects': null,
            'transaction_gas_report': null,
            'replay_cache_summary': null
        };

        // Reset status indicators
        this.updateFileStatus();

        // Process each file
        files.forEach(file => {
            if (file.type === 'application/json' || file.name.endsWith('.json')) {
                this.identifyAndLoadFile(file);
            }
        });
    }

    identifyAndLoadFile(file) {
        const fileName = file.name.toLowerCase();
        let fileType = null;

        if (fileName === 'transaction_data.json') {
            fileType = 'transaction_data';
        } else if (fileName === 'transaction_effects.json') {
            fileType = 'transaction_effects';
        } else if (fileName === 'transaction_gas_report.json') {
            fileType = 'transaction_gas_report';
        } else if (fileName === 'replay_cache_summary.json') {
            fileType = 'replay_cache_summary';
        }

        if (fileType) {
            const reader = new FileReader();
            reader.onload = (e) => {
                try {
                    this.files[fileType] = JSON.parse(e.target.result);
                    this.updateFileStatus();
                    this.updateAnalyzeButton();
                } catch (error) {
                    this.showError(`Error parsing ${fileName}: ${error.message}`);
                }
            };
            reader.readAsText(file);
        }
    }

    updateFileStatus() {
        const statusMap = {
            'transaction_data': 'status-transaction-data',
            'transaction_effects': 'status-transaction-effects',
            'transaction_gas_report': 'status-transaction-gas-report',
            'replay_cache_summary': 'status-replay-cache-summary'
        };

        Object.keys(statusMap).forEach(fileType => {
            const element = document.getElementById(statusMap[fileType]);
            const fileName = fileType.replace('_', '_') + '.json';

            if (this.files[fileType]) {
                element.textContent = `✅ ${fileName}`;
                element.className = 'found';
            } else {
                element.textContent = `❌ ${fileName}`;
                element.className = 'missing';
            }
        });
    }

    handleDragOver(e) {
        e.preventDefault();
    }

    handleDragEnter(e) {
        e.preventDefault();
        document.getElementById('drag-drop-area').classList.add('drag-over');
    }

    handleDragLeave(e) {
        e.preventDefault();
        document.getElementById('drag-drop-area').classList.remove('drag-over');
    }

    handleDrop(e) {
        e.preventDefault();
        document.getElementById('drag-drop-area').classList.remove('drag-over');

        let files = [];

        // Handle directory drop (if items API available)
        if (e.dataTransfer.items) {
            const items = Array.from(e.dataTransfer.items);
            for (const item of items) {
                if (item.kind === 'file') {
                    const entry = item.webkitGetAsEntry();
                    if (entry && entry.isDirectory) {
                        // This is a directory drop
                        this.handleDirectoryDrop(entry);
                        return;
                    }
                }
            }
        }

        // Handle individual file drops
        files = Array.from(e.dataTransfer.files).filter(f =>
            f.type === 'application/json' || f.name.endsWith('.json')
        );

        if (files.length > 0) {
            this.processDirectoryFiles(files);
        }
    }

    handleDirectoryDrop(directoryEntry) {
        const files = [];

        const readDirectory = (dirEntry) => {
            return new Promise((resolve) => {
                const dirReader = dirEntry.createReader();
                dirReader.readEntries((entries) => {
                    const promises = entries.map(entry => {
                        if (entry.isFile && (entry.name.endsWith('.json'))) {
                            return new Promise((resolveFile) => {
                                entry.file((file) => {
                                    files.push(file);
                                    resolveFile();
                                });
                            });
                        }
                        return Promise.resolve();
                    });

                    Promise.all(promises).then(() => resolve());
                });
            });
        };

        readDirectory(directoryEntry).then(() => {
            this.processDirectoryFiles(files);
        });
    }

    updateAnalyzeButton() {
        const allFilesLoaded = Object.values(this.files).every(file => file !== null);
        document.getElementById('analyze-btn').disabled = !allFilesLoaded;
    }

    // Analysis functions ported from Python (keeping all the same logic)

    extractObjectsFromTransactionData(data) {
        const objects = new Set();

        const v1Data = data.V1 || {};
        const ptb = v1Data.kind?.ProgrammableTransaction || {};
        const inputs = ptb.inputs || [];

        // Extract from inputs
        inputs.forEach(inputObj => {
            if (inputObj.Object) {
                if (inputObj.Object.SharedObject) {
                    objects.add(inputObj.Object.SharedObject.id);
                } else if (inputObj.Object.ImmOrOwnedObject) {
                    objects.add(inputObj.Object.ImmOrOwnedObject[0]);
                }
            }
        });

        // Extract from gas payment
        const gasData = v1Data.gas_data || {};
        const payment = gasData.payment || [];
        payment.forEach(paymentObj => {
            objects.add(paymentObj[0]);
        });

        // Extract from commands
        const commands = ptb.commands || [];
        commands.forEach(command => {
            if (command.MoveCall) {
                objects.add(command.MoveCall.package);
            }
        });

        return objects;
    }

    extractGasPaymentObjects(data) {
        const gasObjects = new Set();
        const v1Data = data.V1 || {};
        const gasData = v1Data.gas_data || {};
        const payment = gasData.payment || [];

        payment.forEach(paymentObj => {
            gasObjects.add(paymentObj[0]);
        });

        return gasObjects;
    }

    extractObjectsFromTransactionEffects(data) {
        const objects = new Set();
        const v2Data = data.V2 || {};

        // Changed objects
        const changedObjects = v2Data.changed_objects || [];
        changedObjects.forEach(([objId, _]) => {
            objects.add(objId);
        });

        // Unchanged consensus objects
        const unchangedObjects = v2Data.unchanged_consensus_objects || [];
        unchangedObjects.forEach(([objId, _]) => {
            objects.add(objId);
        });

        return objects;
    }

    analyzeChangedObjectsByOperation(data) {
        const operations = { Created: [], Deleted: [], None: [] };
        const v2Data = data.V2 || {};
        const changedObjects = v2Data.changed_objects || [];

        changedObjects.forEach(([objId, objChange]) => {
            const operation = objChange.id_operation || 'None';
            if (operation === 'Created') {
                operations.Created.push(objId);
            } else if (operation === 'Deleted') {
                operations.Deleted.push(objId);
            } else {
                operations.None.push(objId);
            }
        });

        return operations;
    }

    extractObjectsFromCacheSummary(data) {
        const objects = new Set();
        const cacheEntries = data.cache_entries || [];

        cacheEntries.forEach(entry => {
            objects.add(entry.object_id);
        });

        return objects;
    }

    analyzeCacheByType(data) {
        const cacheAnalysis = { packages: [], objects: [] };
        const cacheEntries = data.cache_entries || [];

        cacheEntries.forEach(entry => {
            if (entry.object_type.Package) {
                cacheAnalysis.packages.push(entry);
            } else {
                cacheAnalysis.objects.push(entry);
            }
        });

        return cacheAnalysis;
    }

    getObjectTypeFromCache(objId, cacheData) {
        const cacheEntries = cacheData.cache_entries || [];

        for (const entry of cacheEntries) {
            if (entry.object_id === objId) {
                if (entry.object_type.Package) {
                    const pkgInfo = entry.object_type.Package;
                    return `Package (${pkgInfo.module_names.length} modules)`;
                } else if (entry.object_type.MoveObject) {
                    const moveObj = entry.object_type.MoveObject;
                    return `${moveObj.module}::${moveObj.name}`;
                } else {
                    return "Unknown";
                }
            }
        }
        return "Not in cache";
    }

    getEnhancedObjectTypeFromCache(objId, cacheData) {
        const cacheEntries = cacheData.cache_entries || [];

        for (const entry of cacheEntries) {
            if (entry.object_id === objId) {
                if (entry.object_type.Package) {
                    const pkgInfo = entry.object_type.Package;
                    return `Package (${pkgInfo.module_names.length} modules)`;
                } else if (entry.object_type.MoveObject) {
                    const moveObj = entry.object_type.MoveObject;
                    // Include package address in the type information
                    return `${moveObj.address}::${moveObj.module}::${moveObj.name}`;
                } else {
                    return "Unknown";
                }
            }
        }
        return "Not in cache";
    }

    getObjectUsage(objId, txDataObjects, txEffectsObjects, gasPaymentObjects) {
        const usage = [];
        if (gasPaymentObjects.has(objId)) {
            usage.push('gas');
        } else if (txDataObjects.has(objId)) {
            usage.push('input');
        } else if (txEffectsObjects.has(objId)) {
            usage.push('runtime');
        }
        return usage;
    }

    getShortObjectTypeFromCache(objId, cacheData) {
        const cacheEntries = cacheData.cache_entries || [];

        for (const entry of cacheEntries) {
            if (entry.object_id === objId) {
                if (entry.object_type.Package) {
                    const pkgInfo = entry.object_type.Package;
                    return `Package (${pkgInfo.module_names.length} modules)`;
                } else if (entry.object_type.MoveObject) {
                    const moveObj = entry.object_type.MoveObject;
                    // Return just module::name (without package address)
                    return `${moveObj.module}::${moveObj.name}`;
                } else {
                    return "Unknown";
                }
            }
        }
        return "Not in cache";
    }

    createTypeWithTooltip(objId, cacheData) {
        try {
            const cacheEntries = cacheData.cache_entries || [];

            for (const entry of cacheEntries) {
                if (entry.object_id === objId) {
                    if (entry.object_type.Package) {
                        // For packages, just return without tooltip since they don't have package addresses
                        const pkgInfo = entry.object_type.Package;
                        return `Package (${pkgInfo.module_names.length} modules)`;
                    } else if (entry.object_type.MoveObject) {
                        const moveObj = entry.object_type.MoveObject;

                        // Short type: module::name
                        const shortType = `${moveObj.module}::${moveObj.name}`;

                        // Full type: address::module::name (with 0x prefix for readability)
                        const fullAddress = moveObj.address.startsWith('0x') ? moveObj.address : `0x${moveObj.address}`;
                        const fullType = `${fullAddress}::${moveObj.module}::${moveObj.name}`;

                        // Use custom tooltip since native tooltips don't work
                        return `<span class="custom-tooltip" data-tooltip="${fullType}" style="cursor: help; text-decoration: underline dotted; color: #87ceeb;">${shortType}</span>`;
                    } else {
                        return "Unknown";
                    }
                }
            }
            return "Not in cache";
        } catch (error) {
            // Fallback to short type
            return this.getShortObjectTypeFromCache(objId, cacheData);
        }
    }

    analyzeTransaction() {
        try {
            this.hideError();

            const data = this.files;

            // Extract objects from each file
            const txDataObjects = this.extractObjectsFromTransactionData(data.transaction_data);
            const txEffectsObjects = this.extractObjectsFromTransactionEffects(data.transaction_effects);
            const gasPaymentObjects = this.extractGasPaymentObjects(data.transaction_data);

            // Get operations for filtering
            const operations = this.analyzeChangedObjectsByOperation(data.transaction_effects);
            const createdObjects = new Set(operations.Created);

            // Store cache data for network detection
            this.cacheData = data.replay_cache_summary;

            // Generate output
            this.renderTransactionOverview(data.transaction_data, data.transaction_effects, data.replay_cache_summary);
            this.renderObjectsTouched(data.replay_cache_summary, txDataObjects, txEffectsObjects, gasPaymentObjects, createdObjects, operations);
            this.renderObjectChanges(operations, txDataObjects, gasPaymentObjects, data.replay_cache_summary);
            this.renderGasAnalysis(data.transaction_gas_report);

            // Create analysis tabs and switch to overview
            this.createAnalysisTabs();
            this.switchToTab('overview');

            // Set up tooltips for all tabs after all rendering is complete
            setTimeout(() => {
                this.setupCustomTooltips();
            }, 100);

        } catch (error) {
            this.showError(`Analysis error: ${error.message}\n${error.stack}`);
        }
    }

    renderTransactionOverview(txData, txEffects, cacheData) {
        const container = document.getElementById('transaction-overview');

        // Extract data
        const v2Effects = txEffects.V2 || {};
        const digest = v2Effects.transaction_digest || 'N/A';
        const epoch = v2Effects.executed_epoch || 'N/A';
        const status = v2Effects.status || 'N/A';
        const checkpoint = cacheData.checkpoint || 'N/A';

        const v1Data = txData.V1 || {};
        const sender = v1Data.sender || 'N/A';
        const kind = v1Data.kind || {};
        const gasData = v1Data.gas_data || {};

        const gasPrice = gasData.price !== undefined ? this.formatNumber(gasData.price) : 'N/A';
        const gasBudget = gasData.budget !== undefined ? this.formatNumber(gasData.budget) : 'N/A';


        // Extract gas summary from transaction effects
        const gasSummary = v2Effects.gas_used || {};
        const computationCost = gasSummary.computationCost !== undefined ? this.formatNumber(gasSummary.computationCost) : 'N/A';
        const storageCost = gasSummary.storageCost !== undefined ? this.formatNumber(gasSummary.storageCost) : 'N/A';
        const storageRebate = gasSummary.storageRebate !== undefined ? this.formatNumber(gasSummary.storageRebate) : 'N/A';
        const nonRefundableStorageFee = gasSummary.nonRefundableStorageFee !== undefined ? this.formatNumber(gasSummary.nonRefundableStorageFee) : 'N/A';

        // Determine status color
        const statusColor = status === 'Success' ? '#90ee90' : '#ff6b6b';

        // Format gas coins table with object id, version, and deletion status
        let gasCoinsTable = '<table id="gas-coins-table" style="width: 100%; border-collapse: collapse; margin-top: 10px;"><thead><tr style="border-bottom: 1px solid #555;"><th style="text-align: left; padding: 5px; color: #4a9eff;">Object ID</th><th style="text-align: left; padding: 5px; color: #4a9eff;">Version</th><th style="text-align: left; padding: 5px; color: #4a9eff;">Status</th></tr></thead><tbody>';

        if ((gasData.payment || []).length > 0) {
            // Get operations to check deletion status
            const operations = this.analyzeChangedObjectsByOperation(txEffects);

            (gasData.payment || []).forEach(paymentObj => {
                const objId = paymentObj[0];
                const version = paymentObj[1] || 'N/A';
                const linkedObj = this.createExplorerLink(objId, 'object');
                const isDeleted = operations.Deleted.includes(objId);
                const status = isDeleted ? '<span style="color: #ff6b6b;">Deleted</span>' : '<span style="color: #90ee90;">Modified</span>';

                gasCoinsTable += `<tr><td style="padding: 5px; font-family: monospace;">${linkedObj}</td><td style="padding: 5px; text-align: right;">${version}</td><td style="padding: 5px;">${status}</td></tr>`;
            });
        } else {
            gasCoinsTable += '<tr><td colspan="3" style="padding: 5px; text-align: center;">N/A</td></tr>';
        }
        gasCoinsTable += '</tbody></table>';

        let html = `
            <p style="margin: 0 0 20px 0; color: #ccc; font-style: italic;">
                This tab provides a quick overall view of the transaction, including basic details, gas costs, and transaction type information.
            </p>
            <div class="overview-section">
                <h4 class="overview-section-title">Transaction Details</h4>
                <div class="overview-subsection">
                    <div class="overview-item">
                        <span class="overview-label">Digest:</span>
                        <span class="overview-value">${this.createExplorerLink(digest, 'txblock')}</span>
                    </div>
                    <div class="overview-item">
                        <span class="overview-label">Sender:</span>
                        <span class="overview-value">${this.createExplorerLink(sender, 'account')}</span>
                    </div>
                    <div class="overview-item">
                        <span class="overview-label">Epoch:</span>
                        <span class="overview-value">${epoch}</span>
                        <span class="overview-label" style="margin-left: 40px;">Checkpoint:</span>
                        <span class="overview-value">${checkpoint}</span>
                    </div>
                    <div class="overview-item">
                        <span class="overview-label">Status:</span>
                        <span class="overview-value" style="color: ${statusColor}; font-weight: bold;">${status}</span>
                    </div>
                </div>
            </div>
            <div class="overview-section">
                <h4 class="overview-section-title">Gas</h4>
                <div class="overview-subsection">
                    <table style="border-collapse: collapse; width: auto;">
                        <tbody>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Price:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${gasPrice}</td>
                            </tr>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Budget:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${gasBudget}</td>
                            </tr>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Computation Cost:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${computationCost}</td>
                            </tr>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Storage Cost:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${storageCost}</td>
                            </tr>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Storage Rebate:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${storageRebate}</td>
                            </tr>
                            <tr>
                                <td style="padding: 4px 8px 4px 0; color: #4a9eff; font-weight: bold;">Non-Refundable Fee:</td>
                                <td style="padding: 4px 0; font-family: monospace; text-align: right;">${nonRefundableStorageFee}</td>
                            </tr>
                        </tbody>
                    </table>
                    <h4 style="margin: 20px 0 10px 0; color: #4a9eff;">Gas Coins:</h4>
                    ${gasCoinsTable}
                </div>
            </div>
        `;

        // Transaction type section (last section with bigger type)
        if (kind.ProgrammableTransaction) {
            const ptb = kind.ProgrammableTransaction;
            const inputs = ptb.inputs || [];
            const commands = ptb.commands || [];

            html += `
                <div class="overview-section">
                    <h4 class="overview-section-title">Transaction Type</h4>
                    <div class="overview-subsection">
                        <div class="overview-item">
                            <span class="overview-label">Type:</span>
                            <span class="overview-value" style="font-size: 1.3em; font-weight: bold;">ProgrammableTransaction</span>
                        </div>
                        <div class="subsection">
                            <h4>Inputs (${inputs.length}):</h4>
                            <div class="scrollable">
            `;

            inputs.forEach((input, i) => {
                let inputDesc = `${i}: `;
                if (input.Object) {
                    if (input.Object.SharedObject) {
                        const shared = input.Object.SharedObject;
                        const linkedId = this.createExplorerLink(shared.id, 'object');
                        inputDesc += `SharedObject: ${linkedId}, mutable=${shared.mutable}`;
                    } else if (input.Object.ImmOrOwnedObject) {
                        const immOwned = input.Object.ImmOrOwnedObject;
                        const linkedId = this.createExplorerLink(immOwned[0], 'object');
                        inputDesc += `ImmOrOwnedObject: ${linkedId}`;
                    }
                } else if (input.Pure) {
                    inputDesc += 'Pure: bytes';
                } else {
                    const keys = Object.keys(input);
                    inputDesc += keys.length > 0 ? keys[0] : 'Unknown';
                }
                html += `<div class="object-item">${inputDesc}</div>`;
            });

            html += `
                        </div>
                        <h4>Commands (${commands.length}):</h4>
                        <div class="scrollable">
            `;

            commands.forEach((command, i) => {
                let cmdDesc = `${i}: `;
                if (command.MoveCall) {
                    const moveCall = command.MoveCall;
                    const linkedPackage = this.createExplorerLink(moveCall.package, 'package');
                    cmdDesc += `MoveCall: ${linkedPackage}::${moveCall.module}::${moveCall.function}`;
                } else if (command.TransferObjects) {
                    cmdDesc += 'TransferObjects';
                } else if (command.SplitCoins) {
                    cmdDesc += 'SplitCoins';
                } else if (command.MergeCoins) {
                    cmdDesc += 'MergeCoins';
                } else {
                    const keys = Object.keys(command);
                    cmdDesc += keys.length > 0 ? keys[0] : 'Unknown';
                }
                html += `<div class="object-item">${cmdDesc}</div>`;
            });

            html += '</div></div>';
            html += `</div>`;
            html += `</div>`;
        } else {
            const keys = Object.keys(kind);
            const txType = keys.length > 0 ? keys[0] : 'Unknown';
            html += `
                <div class="overview-section">
                    <h4 class="overview-section-title">Transaction Type</h4>
                    <div class="overview-subsection">
                        <div class="overview-item">
                            <span class="overview-label">Type:</span>
                            <span class="overview-value" style="font-size: 1.3em; font-weight: bold;">${txType}</span>
                        </div>
                    </div>
                </div>
            `;
        }

        // Dependencies section
        const dependencies = v2Effects.dependencies || [];
        if (dependencies.length > 0) {
            html += `
                <div class="overview-section">
                    <h4 class="overview-section-title">Dependencies (${dependencies.length})</h4>
                    <div class="overview-subsection">
                        <div class="scrollable">
            `;

            dependencies.forEach((depDigest, i) => {
                const linkedDigest = this.createExplorerLink(depDigest, 'txblock');
                html += `<div class="object-item">${i + 1}: ${linkedDigest}</div>`;
            });

            html += `
                        </div>
                    </div>
                </div>
            `;
        }

        container.innerHTML = html;
        this.makeSortable('gas-summary-table');
        this.makeSortable('gas-coins-table');
    }

    renderObjectsTouched(cacheData, txDataObjects, txEffectsObjects, gasPaymentObjects, createdObjects, operations) {
        const container = document.getElementById('objects-touched');
        const cacheAnalysis = this.analyzeCacheByType(cacheData);

        let html = `
            <p style="margin: 0 0 20px 0; color: #ccc; font-style: italic;">
                This section shows all objects and packages involved in the transaction execution. The <strong>Source</strong> indicates how objects were involved: <strong>Input</strong> (transaction arguments), <strong>Gas</strong> (gas payment coins), or <strong>Runtime</strong> (loaded dynamically during execution or created). The <strong>Operation</strong> shows what happened: <strong>Created</strong> (newly generated), <strong>Modified</strong> (state changed), <strong>Deleted</strong> (removed), or <strong>Accessed</strong> (read-only).
            </p>
        `;

        // Packages section
        const filteredPackages = cacheAnalysis.packages.filter(pkg => !createdObjects.has(pkg.object_id));

        html += `<div class="overview-section">`;
        html += `<h3 class="overview-section-title">Packages (${filteredPackages.length})</h3>`;

        // Create packages table
        html += `<table id="packages-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
        html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Package ID</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Version</th><th style="padding: 10px; text-align: center; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Source</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Modules</th></tr></thead>`;
        html += `<tbody>`;

        // Separate and sort packages - input packages first, then dependencies
        const inputPackages = [];
        const dependencyPackages = [];

        filteredPackages.forEach(pkg => {
            const usage = this.getObjectUsage(pkg.object_id, txDataObjects, txEffectsObjects, gasPaymentObjects);
            const pkgType = usage.length > 0 ? 'Input' : 'Dependency';
            if (pkgType === 'Input') {
                inputPackages.push({ pkg, pkgType });
            } else {
                dependencyPackages.push({ pkg, pkgType });
            }
        });

        // Render input packages first, then dependencies
        const allPackages = [...inputPackages, ...dependencyPackages];
        allPackages.forEach(({ pkg, pkgType }) => {
            const pkgInfo = pkg.object_type.Package;
            const linkedPkg = this.createExplorerLink(pkg.object_id, 'package');
            const typeColor = pkgType === 'Input' ? '#90ee90' : '#ccc';

            html += `<tr>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; font-family: monospace;">${linkedPkg}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; text-align: right; font-family: monospace;">${pkg.version}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: ${typeColor}; text-align: center; font-weight: bold;">${pkgType}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; text-align: right; font-family: monospace;">${pkgInfo.module_names.length}</td>`;
            html += `</tr>`;
        });

        html += `</tbody></table>`;
        html += `</div>`;

        // Move Objects section - include all objects (including created ones)
        let filteredObjects = [...cacheAnalysis.objects];


        // Add created objects that aren't already in the cache
        operations.Created.forEach(objId => {
            const existsInCache = filteredObjects.some(obj => obj.object_id === objId);
            if (!existsInCache) {
                // Create a minimal object entry for created objects not in cache
                filteredObjects.push({
                    object_id: objId,
                    version: 'New',
                    object_type: { MoveObject: null } // Will be looked up later
                });
            }
        });


        html += `<div class="overview-section">`;
        html += `<h3 class="overview-section-title">Move Objects (${filteredObjects.length})</h3>`;

        // Create objects table
        html += `<table id="objects-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
        html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Object ID</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Version</th><th style="padding: 10px; text-align: center; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Operation</th><th style="padding: 10px; text-align: center; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Source</th><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Type</th></tr></thead>`;
        html += `<tbody>`;

        // Organize objects with their source, operation, and group by operation
        const modifiedObjectsArray = [];
        const accessedObjectsArray = [];
        const deletedObjectsArray = [];
        const createdObjectsArray = [];

        filteredObjects.forEach(obj => {
            const objId = obj.object_id;

            // Determine source (Input, Gas, or Runtime)
            let objSource = 'Runtime';
            if (txDataObjects.has(objId)) {
                objSource = gasPaymentObjects.has(objId) ? 'Gas' : 'Input';
            }

            // Determine operation
            let objOperation = 'Accessed';
            if (operations.Created.includes(objId)) {
                objOperation = 'Created';
                createdObjectsArray.push({ obj, objSource, objOperation });
            } else if (operations.Deleted.includes(objId)) {
                objOperation = 'Deleted';
                deletedObjectsArray.push({ obj, objSource, objOperation });
            } else if (operations.None.includes(objId)) {
                objOperation = 'Modified';
                modifiedObjectsArray.push({ obj, objSource, objOperation });
            } else {
                accessedObjectsArray.push({ obj, objSource, objOperation });
            }
        });

        // Render in order: Modified, Accessed, Deleted, Created
        const allObjects = [...modifiedObjectsArray, ...accessedObjectsArray, ...deletedObjectsArray, ...createdObjectsArray];


        allObjects.forEach(({ obj, objSource, objOperation }) => {
            const objType = this.createTypeWithTooltip(obj.object_id, cacheData);
            const linkedObj = this.createExplorerLink(obj.object_id, 'object');
            const sourceColor = objSource === 'Input' ? '#90ee90' : objSource === 'Gas' ? '#ffd700' : '#87ceeb';
            const operationColor = objOperation === 'Created' ? '#87ceeb' : objOperation === 'Deleted' ? '#ff6b6b' : objOperation === 'Modified' ? '#dda0dd' : '#ccc';

            html += `<tr>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; font-family: monospace;">${linkedObj}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; text-align: right; font-family: monospace;">${obj.version}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: ${operationColor}; text-align: center; font-weight: bold;">${objOperation}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: ${sourceColor}; text-align: center; font-weight: bold;">${objSource}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; font-family: monospace;">${objType}</td>`;
            html += `</tr>`;
        });

        html += `</tbody></table>`;
        html += `</div>`;
        container.innerHTML = html;
        this.makeSortable('packages-table');
        this.makeSortable('objects-table');
    }

    renderObjectChanges(operations, txDataObjects, gasPaymentObjects, cacheData) {
        const container = document.getElementById('object-changes');
        let html = `
            <p style="margin: 0 0 20px 0; color: #ccc; font-style: italic;">
                This section shows the actual state changes captured in transaction effects. The <strong>Operation</strong> indicates what happened: <strong>Created</strong> (newly generated), <strong>Modified</strong> (state changed), or <strong>Deleted</strong> (destroyed). The <strong>Runtime</strong> column shows the object's relationship to the transaction: <strong>Input</strong> (transaction arguments), <strong>Gas</strong> (payment coins), or <strong>Runtime</strong> (created during execution or loaded dynamically).
            </p>
        `;

        // Object Changes Table
        html += `<div class="overview-section">`;
        html += `<h3 style="color: white; margin: 15px 0 10px 0; font-size: 1.3em; font-weight: bold;">Object Changes</h3>`;

        // Create unified object changes table
        html += `<table id="object-changes-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
        html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Object ID</th><th style="padding: 10px; text-align: center; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Operation</th><th style="padding: 10px; text-align: center; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Source</th><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Type</th></tr></thead>`;
        html += `<tbody>`;

        // Collect all objects with their operations and kinds
        const allChangedObjects = [];

        // Created objects
        operations.Created.forEach(objId => {
            const objType = this.createTypeWithTooltip(objId, cacheData);
            allChangedObjects.push({ objId, operation: 'Created', kind: 'Runtime', objType });
        });

        // Deleted objects
        operations.Deleted.forEach(objId => {
            let objKind = 'Runtime';
            if (txDataObjects.has(objId)) {
                objKind = gasPaymentObjects.has(objId) ? 'Gas' : 'Input';
            }
            const objType = this.createTypeWithTooltip(objId, cacheData);
            allChangedObjects.push({ objId, operation: 'Deleted', kind: objKind, objType });
        });

        // Modified objects
        operations.None.forEach(objId => {
            let objKind = 'Runtime';
            if (txDataObjects.has(objId)) {
                objKind = gasPaymentObjects.has(objId) ? 'Gas' : 'Input';
            }
            const objType = this.createTypeWithTooltip(objId, cacheData);
            allChangedObjects.push({ objId, operation: 'Modified', kind: objKind, objType });
        });

        allChangedObjects.forEach(({ objId, operation, kind, objType }) => {
            const linkedObj = this.createExplorerLink(objId, 'object');
            const operationColor = operation === 'Created' ? '#87ceeb' : operation === 'Deleted' ? '#ff6b6b' : '#dda0dd';
            const kindColor = kind === 'Input' ? '#90ee90' : kind === 'Gas' ? '#ffd700' : kind === 'New' ? '#87ceeb' : '#87ceeb';

            html += `<tr>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; font-family: monospace;">${linkedObj}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: ${operationColor}; text-align: center; font-weight: bold;">${operation}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: ${kindColor}; text-align: center; font-weight: bold;">${kind}</td>`;
            html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white; font-family: monospace;">${objType}</td>`;
            html += `</tr>`;
        });

        html += `</tbody></table>`;
        html += `</div>`;
        container.innerHTML = html;
        this.makeSortable('object-changes-table');
    }

    renderGasAnalysis(gasReport) {
        const container = document.getElementById('gas-analysis');

        if (!gasReport) {
            container.innerHTML = '<p>Gas report not available</p>';
            return;
        }

        let html = `
            <p style="margin: 0 0 20px 0; color: #ccc; font-style: italic;">
                This section provides detailed gas analysis including gas constants, cost breakdown, and per-object gas usage. It shows exactly how gas was consumed during transaction execution, including storage costs and rebates for each object touched.
            </p>
        `;

        // Gas Constants
        html += `<div class="overview-section">`;
        html += `<h3 class="overview-section-title">Gas Constants</h3>`;
        html += `<div style="display: flex; gap: 30px; font-family: monospace;">`;

        if (gasReport.reference_gas_price !== undefined) {
            html += `<span style="color: white;">Reference Gas Price: <span style="color: white;">${this.formatNumber(gasReport.reference_gas_price)}</span></span>`;
        }

        if (gasReport.storage_gas_price !== undefined) {
            html += `<span style="color: white;">Storage Gas Price: <span style="color: white;">${this.formatNumber(gasReport.storage_gas_price)}</span></span>`;
        }

        if (gasReport.rebate_rate !== undefined) {
            html += `<span style="color: white;">Rebate Rate: <span style="color: white;">${gasReport.rebate_rate / 100}%</span></span>`;
        }

        html += `</div>`;
        html += `</div>`;

        // Gas Summary - transaction specific
        html += `<div class="overview-section">`;
        html += `<h3 class="overview-section-title">Gas Summary</h3>`;
        html += `<table id="gas-summary-table" style="width: 70%; border-collapse: collapse;">`;

        if (gasReport.gas_price !== undefined) {
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Gas Price</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(gasReport.gas_price)}</td></tr>`;
        }

        if (gasReport.gas_budget !== undefined) {
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Gas Budget</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(gasReport.gas_budget)}</td></tr>`;
        }

        if (gasReport.gas_used !== undefined) {
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Gas Used</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(gasReport.gas_used)}</td></tr>`;
        }
        if (gasReport.cost_summary) {
            const costSummary = gasReport.cost_summary;
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Computation Cost</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(costSummary.computationCost)}</td></tr>`;
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Storage Cost</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(costSummary.storageCost)}</td></tr>`;
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Storage Rebate</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(costSummary.storageRebate)}</td></tr>`;
            html += `<tr><td style="padding: 6px 15px; border-bottom: 1px solid #333; color: white;">Non-Refundable Storage Fee</td><td style="padding: 6px 15px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(costSummary.nonRefundableStorageFee)}</td></tr>`;
        }

        html += `</table>`;
        html += `</div>`;

        // Per-Object Storage Breakdown
        if (gasReport.per_object_storage && gasReport.per_object_storage.length > 0) {
            html += `<div class="overview-section">`;
            html += `<h3 class="overview-section-title">Per-Object Storage Breakdown</h3>`;

            // Categorize objects
            const deletedGasObjects = [];
            const createdGasObjects = [];
            const modifiedGasObjects = [];

            gasReport.per_object_storage.forEach(([objectId, storageInfo]) => {
                const storageCost = parseInt(storageInfo.storage_cost || '0');
                const storageRebate = parseInt(storageInfo.storage_rebate || '0');
                const newSize = parseInt(storageInfo.new_size || '0');

                if (newSize === 0 && storageRebate > 0) {
                    deletedGasObjects.push([objectId, storageInfo]);
                } else if (storageCost > 0 && storageRebate === 0) {
                    createdGasObjects.push([objectId, storageInfo]);
                } else {
                    modifiedGasObjects.push([objectId, storageInfo]);
                }
            });

            // Deleted Objects
            if (deletedGasObjects.length > 0) {
                html += `<h3 style="color: white; margin: 15px 0 10px 0; font-size: 1.1em;">Deleted Objects</h3>`;
                html += `<table id="gas-deleted-objects-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
                html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Object ID</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Storage Rebate</th></tr></thead>`;
                html += `<tbody>`;

                deletedGasObjects.forEach(([objectId, storageInfo]) => {
                    html += `<tr>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white;">${this.createExplorerLink(objectId, 'object')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(storageInfo.storage_rebate || '0')}</td>`;
                    html += `</tr>`;
                });

                html += `</tbody></table>`;
            }

            // Created Objects
            if (createdGasObjects.length > 0) {
                html += `<h3 style="color: white; margin: 15px 0 10px 0; font-size: 1.1em;">Created Objects</h3>`;
                html += `<table id="gas-created-objects-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
                html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Object ID</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Size (bytes)</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Storage Cost</th></tr></thead>`;
                html += `<tbody>`;

                createdGasObjects.forEach(([objectId, storageInfo]) => {
                    html += `<tr>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white;">${this.createExplorerLink(objectId, 'object')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(storageInfo.new_size || '0')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(storageInfo.storage_cost || '0')}</td>`;
                    html += `</tr>`;
                });

                html += `</tbody></table>`;
            }

            // Modified Objects
            if (modifiedGasObjects.length > 0) {
                html += `<h3 style="color: white; margin: 15px 0 10px 0; font-size: 1.1em;">Modified Objects</h3>`;
                html += `<table id="gas-modified-objects-table" style="width: 100%; border-collapse: collapse; margin-bottom: 20px;">`;
                html += `<thead><tr style="background: #333;"><th style="padding: 10px; text-align: left; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Object ID</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Size (bytes)</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Storage Cost</th><th style="padding: 10px; text-align: right; color: #4a9eff; border-bottom: 2px solid #4a9eff;">Non-Refundable</th></tr></thead>`;
                html += `<tbody>`;

                // Get rebate rate from gas report
                const rebateRate = gasReport.rebate_rate || 9900; // Default to 99% if not found

                modifiedGasObjects.forEach(([objectId, storageInfo]) => {
                    const storageCost = parseInt(storageInfo.storage_cost || '0');

                    // Calculate non-refundable: cost minus (cost * rebate_rate / 10000)
                    const nonRefundable = storageCost - Math.floor(storageCost * rebateRate / 10000);

                    html += `<tr>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; color: white;">${this.createExplorerLink(objectId, 'object')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(storageInfo.new_size || '0')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(storageInfo.storage_cost || '0')}</td>`;
                    html += `<td style="padding: 8px; border-bottom: 1px solid #333; text-align: right; font-family: monospace; color: white;">${this.formatNumber(nonRefundable)}</td>`;
                    html += `</tr>`;
                });

                html += `</tbody></table>`;
            }
            html += `</div>`;
        }

        container.innerHTML = html;
        this.makeSortable('gas-deleted-objects-table');
        this.makeSortable('gas-created-objects-table');
        this.makeSortable('gas-modified-objects-table');
    }

    formatOwner(owner) {
        if (typeof owner === 'string') {
            return this.createExplorerLink(owner, 'account');
        } else if (owner && owner.AddressOwner) {
            return this.createExplorerLink(owner.AddressOwner, 'account');
        } else if (owner && owner.ObjectOwner) {
            return this.createExplorerLink(owner.ObjectOwner, 'object');
        } else if (owner === 'Immutable') {
            return 'Immutable';
        } else if (owner && owner.Shared) {
            return `Shared (initial_shared_version: ${owner.Shared.initial_shared_version})`;
        }
        return JSON.stringify(owner);
    }

    escapeHtml(text) {
        // Use the more comprehensive HTML encoding method
        return this.encodeHTML(text);
    }

    showError(message) {
        document.getElementById('error-message').textContent = message;

        // Create error tab if it doesn't exist
        const tabNav = document.getElementById('tab-nav');
        if (!document.querySelector('[data-tab="error"]')) {
            const errorTab = document.createElement('button');
            errorTab.className = 'tab-btn';
            errorTab.setAttribute('data-tab', 'error');
            errorTab.textContent = 'Error';
            errorTab.style.color = '#ff6b6b';
            tabNav.appendChild(errorTab);
        }

        // Switch to error tab
        this.switchToTab('error');
    }

    hideError() {
        // Remove error tab if it exists
        const errorTab = document.querySelector('[data-tab="error"]');
        if (errorTab) {
            errorTab.remove();
        }
    }
}

// Initialize the viewer when page loads
document.addEventListener('DOMContentLoaded', () => {
    new TransactionViewer();
});