import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Link from "next/link";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: { default: "Mining Pool", template: "%s · Mining Pool" },
  description: "Multi-coin mining pool",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}>
      <body className="flex min-h-full flex-col font-sans">
        <header className="border-b border-border bg-surface">
          <nav className="mx-auto flex w-full max-w-5xl items-center gap-6 px-4 py-3">
            <Link href="/" className="font-semibold">
              Mining Pool
            </Link>
            <Link href="/" className="text-sm text-muted hover:text-foreground">
              Dashboard
            </Link>
            <Link href="/blocks" className="text-sm text-muted hover:text-foreground">
              Blocks
            </Link>
          </nav>
        </header>
        <main className="mx-auto flex w-full max-w-5xl flex-1 flex-col gap-4 px-4 py-6">{children}</main>
      </body>
    </html>
  );
}
