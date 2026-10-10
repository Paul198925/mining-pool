import type { Metadata } from "next";
import Link from "next/link";

import { logout } from "../actions";

export const metadata: Metadata = {
  title: { default: "Admin", template: "%s · Admin" },
  robots: { index: false, follow: false },
};

const PAGES = [
  { href: "/admin", label: "Overview" },
  { href: "/admin/payouts", label: "Payouts" },
  { href: "/admin/blocks", label: "Blocks" },
  { href: "/admin/miners", label: "Miners" },
  { href: "/admin/servers", label: "Servers" },
];

export default function AdminLayout({ children }: LayoutProps<"/admin">) {
  return (
    <div className="column">
      <nav className="subnav">
        <b>Admin</b>
        {PAGES.map((page) => (
          <Link key={page.href} href={page.href}>
            {page.label}
          </Link>
        ))}
        <form action={logout} className="ml-auto">
          <button type="submit" className="button">
            Log out
          </button>
        </form>
      </nav>
      {children}
    </div>
  );
}
