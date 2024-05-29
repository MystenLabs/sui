// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Action, blocklist } from '../background/blocklist';

function generateWarningPage(elementId: string) {
	return `
	<div class="${elementId}-container">
		<div class="${elementId}-title">
			<img src="https://assets-global.website-files.com/6425f546844727ce5fb9e5ab/65690e5e73e9e2a416e3502f_sui-mark.svg" class="${elementId}-sui-mark" />
			<img src="https://assets-global.website-files.com/6425f546844727ce5fb9e5ab/65690e9a6e0d07d1b68c7050_sui-type.svg" class="${elementId}-sui-type" />
			<span>Deceptive site ahead</span>
		</div>
		<p class="${elementId}-description">SuiSec flagged the site you're trying to visit as potentially deceptive. Attackers may trick you into doing something dangerous.</p>
		<ul class="${elementId}-list">
			<li>Secret Recovery Phrase or password theft</li>
			<li>Malicious transactions resulting in stolen assets</li>
			<li>If you understand the risks and still want to proceed, you can continue to the site</li>
		</ul>
		<button class="${elementId}-button">continue</button>
	</div>
	`;
}

function generatePageCss(elementId: string) {
	return `
	.${elementId}-container {
		width: 100vw;
		position: fixed;
		top: 0;
		left: 0;
		padding: 30px 0 24px;
		box-sizing: border-box;
		z-index: 100000;
		background: #E3493F;
    	color: #FFF;
	}
	.${elementId}-title {
		height: 32px;
		line-height: 32px;
		display: flex;
		align-items: center;
	}
	.${elementId}-sui-mark {
		width: 28px;
	}
	.${elementId}-sui-type {
		width: 38px;
	}
	.${elementId}-title span {
	    font-weight: 700;
		font-size: 24px;
		line-height: 24px;
		margin-left: 18px;
	}
	.${elementId}-description {
		margin: 16px 0 8px;
		font-weight: 700;
		font-size: 18px;
		line-height: 24px;
	}
	.${elementId}-list {
		list-style-type: disc;
		font-weight: 400;
		font-size: 16px;
		line-height: 20px;
	}
	.${elementId}-button {
		padding: 4px 8px;
		font-weight: 400;
		font-size: 12px;
		line-height: 16px;
		color: #fff;
		border-radius: 4px;
		border: 1px solid #fff;
		background: transparent;
		outline: none;
		margin-top: 16px;
		cursor: pointer;
	}
	@media screen and (min-width: 1316px) {
		.${elementId}-container {
		  	padding: 30px calc(50vw - 614px) 24px;
		}
	}
  	@media screen and (max-width: 1316px) {
		.${elementId}-container {
		  	padding: 30px 44px 24px;
		}
	}
  	@media screen and (max-width: 768px) {
		.${elementId}-container {
		  	padding: 30px 44px 24px;
		}
	}
  	@media screen and (max-width: 375px) {
		.${elementId}-container {
		  	padding: 30px 16px 24px;
		}
	}
	`;
}

function generateCamelCaseId() {
	const letters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';

	// Randomly select four letters
	const firstPart = letters.charAt(Math.floor(Math.random() * letters.length));
	const secondPart = letters.charAt(Math.floor(Math.random() * letters.length));
	const thirdPart = letters.charAt(Math.floor(Math.random() * letters.length));
	const fourthPart = letters.charAt(Math.floor(Math.random() * letters.length));

	// Returns a four-digit camelCase ID
	return firstPart + secondPart.toLowerCase() + thirdPart + fourthPart.toLowerCase();
}

export function domainScanning() {
	blocklist.scanDomain(window.location.href).then((action) => {
		if (action === Action.BLOCK) {
			document.addEventListener('DOMContentLoaded', function () {
				const bodyElement = document.body;
				const warningPageElement = document.createElement('div');
				warningPageElement.id = generateCamelCaseId();
				const shadowRoot = warningPageElement.attachShadow({ mode: 'open' });

				// Add warning page
				const elementId = generateCamelCaseId();
				const warningPage = document.createElement('div');
				warningPage.id = elementId;
				warningPage.innerHTML = generateWarningPage(elementId);
				shadowRoot.appendChild(warningPage);

				// Add <style>
				const styleElement = document.createElement('style');
				styleElement.textContent = generatePageCss(elementId);
				shadowRoot.appendChild(styleElement);
				bodyElement.appendChild(warningPageElement);

				// Get the button element and add a click event listener
				const closeButton = shadowRoot.querySelector(`.${elementId}-button`) as HTMLElement;
				closeButton.addEventListener('click', function () {
					// hide warning page
					const container = shadowRoot.querySelector(`.${elementId}-container`) as HTMLElement;
					container.style.display = 'none';
				});
			});
		}
	});
}
