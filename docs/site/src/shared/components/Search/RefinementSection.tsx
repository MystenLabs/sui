
import React from "react";
import { useRefinementList, useHits } from "react-instantsearch";

export default function RefinementSection() {
  const { hits } = useHits();
  const { items, refine } = useRefinementList({ attribute: "source" });

  if (hits.length === 0) return null;

  return (
    <div className="col-span-12 md:col-span-4 xl:col-span-3">
      <div className="sticky mr-4 p-6 pb-[40px] top-24 z-10 border border-solid border-sui-gray-55 rounded-[20px]">
        <h2 className="text-lg font-semibold text-sui-gray-3s dark:text-sui-gray-45">
          Refine results
        </h2>
        <ul className="pl-0">
          {items.map((item) => (
            <li
              key={item.label}
              className="dark:text-sui-gray-45 mb-2 flex justify-between items-center w-full"
            >
              <div className="flex items-center space-x-2">
                <label className="flex items-center space-x-2 text-sm cursor-pointer">
                  <input
                    type="checkbox"
                    checked={item.isRefined}
                    onChange={() => refine(item.value)}
                    className="sr-only peer"
                  />
                  <div className="flex peer-checked:hidden peer-checked:bg-sui-primary rounded !ml-0 dark:bg-sui-gray-35">
                    <svg
                      width="20"
                      height="20"
                      viewBox="0 0 20 20"
                      fill="none"
                      xmlns="http://www.w3.org/2000/svg"
                      className="dark:text-sui-gray-35"
                    >
                      <rect
                        x="0.5"
                        y="0.5"
                        width="19"
                        height="19"
                        rx="3.5"
                        stroke="black"
                        stroke-opacity="0.4"
                      />
                      <path
                        opacity="0.2"
                        d="M13.4485 6.24987L8.39343 11.8514L6.55151 9.81037C6.40001 9.64823 6.1971 9.55851 5.98648 9.56053C5.77587 9.56256 5.57439 9.65618 5.42546 9.82121C5.27653 9.98625 5.19205 10.2095 5.19022 10.4429C5.18839 10.6763 5.26935 10.9011 5.41568 11.069L7.82551 13.7394C7.97615 13.9063 8.18043 14 8.39343 14C8.60643 14 8.81071 13.9063 8.96135 13.7394L14.5843 7.50851C14.7306 7.34063 14.8116 7.11578 14.8098 6.88239C14.8079 6.649 14.7234 6.42575 14.5745 6.26071C14.4256 6.09568 14.2241 6.00206 14.0135 6.00003C13.8029 5.99801 13.6 6.08773 13.4485 6.24987Z"
                        fill="#030F1C"
                      />
                    </svg>
                  </div>
                  <div className="hidden peer-checked:flex peer-checked:bg-sui-primary rounded !ml-0 dark:bg-sui-gray-35">
                    <svg
                      width="20"
                      height="20"
                      viewBox="0 0 20 20"
                      fill="none"
                      xmlns="http://www.w3.org/2000/svg"
                    >
                      <rect
                        x="0.5"
                        y="0.5"
                        width="19"
                        height="19"
                        rx="3.5"
                        stroke="black"
                      />
                      <path
                        d="M13.4485 6.24987L8.39343 11.8514L6.55151 9.81037C6.40001 9.64823 6.1971 9.55851 5.98648 9.56053C5.77587 9.56256 5.57439 9.65618 5.42546 9.82121C5.27653 9.98625 5.19205 10.2095 5.19022 10.4429C5.18839 10.6763 5.26935 10.9011 5.41568 11.069L7.82551 13.7394C7.97615 13.9063 8.18043 14 8.39343 14C8.60643 14 8.81071 13.9063 8.96135 13.7394L14.5843 7.50851C14.7306 7.34063 14.8116 7.11578 14.8098 6.88239C14.8079 6.649 14.7234 6.42575 14.5745 6.26071C14.4256 6.09568 14.2241 6.00206 14.0135 6.00003C13.8029 5.99801 13.6 6.08773 13.4485 6.24987Z"
                        fill="#030F1C"
                      />
                    </svg>
                  </div>
                  <span className="text-sui-gray-3s dark:text-sui-gray-45 peer-checked:text-sui-gray-5s peer-checked:font-bold dark:peer-checked:text-sui-gray-35">
                    {item.label}
                  </span>
                </label>
              </div>
              <span className="text-sm text-gray-500 dark:text-sui-gray-35">
                {item.count}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
