"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const TABS = [
  { href: "/", label: "Home" },
  { href: "/pool", label: "Pool" },
  { href: "/wallet", label: "Wallet" },
  { href: "/graphs", label: "Graphs" },
];

/** `pathname` is null while the static shell renders, which simply leaves no tab highlighted. */
export function NavTabs({ pathname }: { pathname?: string | null }) {
  return (
    <nav className="tabmenu">
      <Link href="/" className="site">
        Mining Pool
      </Link>
      {TABS.map((tab) => {
        const selected = pathname != null && (tab.href === "/" ? pathname === "/" : pathname.startsWith(tab.href));
        return (
          <Link key={tab.href} href={tab.href} className={selected ? "selected" : undefined}>
            {tab.label}
          </Link>
        );
      })}
    </nav>
  );
}

export function ActiveNavTabs() {
  return <NavTabs pathname={usePathname()} />;
}
