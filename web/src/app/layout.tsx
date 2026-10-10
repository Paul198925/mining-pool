import type { Metadata } from "next";
import { Suspense } from "react";

import { ActiveNavTabs, NavTabs } from "@/components/nav-tabs";

import "./globals.css";

export const metadata: Metadata = {
  title: { default: "Mining Pool", template: "%s · Mining Pool" },
  description: "Multi-coin mining pool",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className="h-full">
      <body className="flex min-h-full flex-col">
        <Suspense fallback={<NavTabs />}>
          <ActiveNavTabs />
        </Suspense>
        <main className="page-body">{children}</main>
        <footer className="footer">
          Mining Pool · <a href="/api/pool">API</a>
        </footer>
      </body>
    </html>
  );
}
