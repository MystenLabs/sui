// Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// Enables the Push Feedback plugin at the footer of each docs page
// https://app.pushfeedback.com/
// Code from Push Feedback documentation: https://docs.pushfeedback.com/installation/docusaurus/#advanced-setup-like-and-dislike-buttons
// Currently uses the 'free' tier of the plugin

import React, {useEffect} from 'react';
import Footer from '@theme-original/DocItem/Footer';

import { FeedbackButton } from 'pushfeedback-react';
import { defineCustomElements } from 'pushfeedback/loader';
import 'pushfeedback/dist/pushfeedback/pushfeedback.css';

function FeedbackWidget() {
    const buttonThumbsUp = <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3"></path></svg>;
    const buttonThumbsDown = <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M10 15v4a3 3 0 0 0 3 3l4-9V2H5.72a2 2 0 0 0-2 1.7l-1.38 9a2 2 0 0 0 2 2.3zm7-13h2.67A2.31 2.31 0 0 1 22 4v7a2.31 2.31 0 0 1-2.33 2H17"></path></svg>;
    const projectId = 'yap67az1qz';

    useEffect(() => {
        if (typeof window !== 'undefined') {
            defineCustomElements(window);
        }
    }, []);

    return(
        <div className="feedback-widget margin-top--md margin-bottom--md">
            <div className="margin-bottom--sm">
                <b>Was this helpful?</b>
            </div>
            <span className="feedback-widget-positive">
                <FeedbackButton project={projectId} rating="1" custom-font="True" button-style="default" modal-position="center">
                    <button className="button button--outline button--primary button--sm" title="Yes">
                        {buttonThumbsUp}
                    </button>
                </FeedbackButton>
            </span>
            <span className="feedback-widget-negative margin-left--sm">
                <FeedbackButton project={projectId} rating="0" custom-font="True" button-style="default" modal-position="center">
                    <button className="button button--outline button--primary button--sm" title="No">
                        {buttonThumbsDown}
                    </button>
                </FeedbackButton>
            </span>
        </div>
    );
}

export default function FooterWrapper(props) {
    return (
        <>
        <FeedbackWidget/>
        <Footer {...props} />
        </>
    );
}