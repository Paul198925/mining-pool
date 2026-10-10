"use client";

import { type ReactNode, useState } from "react";

export type PagedColumn = { label: string; right?: boolean; title?: string };
/** `text` is what the filter searches; `cells` is what is shown. */
export type PagedRow = { key: string; text: string; cells: ReactNode[] };

const PAGE_SIZES = [10, 25, 50];

/** A table with a filter field and paging, for lists that can get long. */
export function PagedTable({ columns, rows, empty }: { columns: PagedColumn[]; rows: PagedRow[]; empty: string }) {
  const [filter, setFilter] = useState("");
  const [size, setSize] = useState(PAGE_SIZES[0]);
  const [page, setPage] = useState(0);

  const needle = filter.trim().toLowerCase();
  const matching = needle ? rows.filter((row) => row.text.toLowerCase().includes(needle)) : rows;
  const pages = Math.max(1, Math.ceil(matching.length / size));
  // Rows can disappear on a refresh; never show a page that no longer exists.
  const current = Math.min(page, pages - 1);
  const shown = matching.slice(current * size, current * size + size);
  const from = matching.length === 0 ? 0 : current * size + 1;

  if (rows.length === 0) {
    return <p className="empty">{empty}</p>;
  }
  return (
    <div>
      <div className="flex p-1">
        <input
          className="text-input"
          value={filter}
          placeholder="Type to filter"
          aria-label="Filter the table"
          spellCheck={false}
          onChange={(event) => {
            setFilter(event.target.value);
            setPage(0);
          }}
        />
      </div>
      <table className="datagrid">
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.label} className={column.right ? "right" : undefined} title={column.title}>
                {column.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((row) => (
            <tr key={row.key}>
              {row.cells.map((cell, index) => (
                <td key={index} className={columns[index]?.right ? "right" : undefined}>
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex flex-wrap items-center justify-end gap-3 p-1">
        <label>
          Items per page:{" "}
          <select
            value={size}
            onChange={(event) => {
              setSize(Number(event.target.value));
              setPage(0);
            }}
          >
            {PAGE_SIZES.map((option) => (
              <option key={option}>{option}</option>
            ))}
          </select>
        </label>
        <span>
          {from}–{Math.min(matching.length, current * size + size)} of {matching.length}
        </span>
        <span className="flex gap-1">
          <button type="button" className="button" disabled={current === 0} onClick={() => setPage(current - 1)} aria-label="Previous page">
            ‹
          </button>
          <button
            type="button"
            className="button"
            disabled={current >= pages - 1}
            onClick={() => setPage(current + 1)}
            aria-label="Next page"
          >
            ›
          </button>
        </span>
      </div>
    </div>
  );
}
