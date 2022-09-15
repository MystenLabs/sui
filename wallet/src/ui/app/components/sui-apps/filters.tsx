// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useEffect, useState } from 'react';
import ReactDOM from 'react-dom';
import { NavLink } from 'react-router-dom';

import st from './Filters.module.scss';

const ELEMENT_ID = '#sui-apps-filters';
function activeTagsFilter({ isActive }: { isActive: boolean }) {
    return cl({ [st.active]: isActive }, st.filter);
}

function AppFiltersPortal() {
    const [ready, setReady] = useState(false);
    const content = document.querySelector(ELEMENT_ID) as HTMLElement;

    const filterTags = [
        {
            name: 'Playground',
            link: 'apps',
        },
        {
            name: 'Active Connections',
            link: 'apps/connected',
        },
    ];

    useEffect(() => {
        // TODO - Remove this hack
        const content = document.querySelector(ELEMENT_ID) as HTMLElement;
        if (content) setReady(true);
    }, [content]);

    return (
        <>
            {ready
                ? ReactDOM.createPortal(
                      <div className={st.filterTags}>
                          {filterTags.map((tag) => (
                              <NavLink
                                  key={tag.link}
                                  to={`/${tag.link}`}
                                  end
                                  className={activeTagsFilter}
                                  title={tag.name}
                              >
                                  <span className={st.title}>{tag.name}</span>
                              </NavLink>
                          ))}
                      </div>,
                      content
                  )
                : null}
        </>
    );
}

export default AppFiltersPortal;
