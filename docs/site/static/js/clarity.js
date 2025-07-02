// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

(function() {
  function initClarity() {
    (function(c,l,a,r,i,t,y){
      c[a]=c[a]||function(){(c[a].q=c[a].q||[]).push(arguments)};
      t=l.createElement(r);t.async=1;t.src="https://www.clarity.ms/tag/"+i;
      t.onerror = function() { console.warn('Failed to load Microsoft Clarity'); };
      y=l.getElementsByTagName(r)[0];y.parentNode.insertBefore(t,y);
    })(window, document, "clarity", "script", "s0jycwr855");
  }
  
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initClarity);
  } else {
    initClarity();
  }
})(); 