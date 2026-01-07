/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import styles from './styles.module.css';

export default function SidebarIframe({ 
  url, 
  label = "External Link",
  icon = "🔗",
  openInNewTab = false  // Add this prop
}) {
  const [isOpen, setIsOpen] = useState(false);

  // Handle ESC key to close modal
  useEffect(() => {
    const handleEsc = (event) => {
      if (event.key === 'Escape') {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleEsc);
      document.body.style.overflow = 'hidden';
    }

    return () => {
      document.removeEventListener('keydown', handleEsc);
      document.body.style.overflow = 'unset';
    };
  }, [isOpen]);

  // If openInNewTab is true, render as a link
  if (openInNewTab) {
    return (
      <a 
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        className={styles.sidebarButton}
        style={{ textDecoration: 'none' }}
      >
        <span className={styles.icon}>{icon}</span>
        <span className={styles.label}>{label}</span>
        <span className={styles.arrow}>→</span>
      </a>
    );
  }

  // Otherwise render as modal button
  return (
    <>
      <button 
        className={styles.sidebarButton}
        onClick={() => setIsOpen(true)}
      >
        <span className={styles.icon}>{icon}</span>
        <span className={styles.label}>{label}</span>
        <span className={styles.arrow}>→</span>
      </button>

      {isOpen && createPortal(
        <div className={styles.modalOverlay} onClick={() => setIsOpen(false)}>
          <div className={styles.modalContent} onClick={(e) => e.stopPropagation()}>
            <div className={styles.modalHeader}>
              <h3>{label}</h3>
              <button 
                className={styles.closeButton}
                onClick={() => setIsOpen(false)}
                aria-label="Close"
              >
                ✕
              </button>
            </div>
            <div className={styles.iframeContainer}>
              <iframe
                src={url}
                width="100%"
                height="700px"
                title={label}
                frameBorder="0"
                allowFullScreen
              />
            </div>
          </div>
        </div>,
        document.body
      )}
    </>
  );
}