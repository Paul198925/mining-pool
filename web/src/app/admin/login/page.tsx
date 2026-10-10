import type { Metadata } from "next";

import { Box } from "@/components/ui";

import { LoginForm } from "./login-form";

export const metadata: Metadata = { title: "Admin login", robots: { index: false, follow: false } };

export default function LoginPage() {
  return (
    <div className="columns">
      <Box title="ADMIN LOGIN">
        <LoginForm />
      </Box>
    </div>
  );
}
