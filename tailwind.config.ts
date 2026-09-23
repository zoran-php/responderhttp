// http_client/tailwind.config.ts
import type { Config } from "tailwindcss";

// shadcn/ui-compatible token setup: colours resolve to CSS variables defined
// in src/index.css so light/dark themes stay in one place. Do not add ad-hoc
// hex values here — extend the variable set in index.css instead.
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    container: {
      center: true,
      padding: "2rem",
      screens: { "2xl": "1400px" },
    },
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        // Nord Ice's accent-orange (#d08770) — defined alongside the other
        // tokens so it has a home, not yet consumed by any component.
        warning: {
          DEFAULT: "hsl(var(--warning))",
          foreground: "hsl(var(--warning-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        // One color per HTTP method, consumed via
        // lib/http-method-colors.ts rather than referenced by name in
        // components, so the method → color mapping has one home.
        method: {
          get: "hsl(var(--method-get))",
          post: "hsl(var(--method-post))",
          put: "hsl(var(--method-put))",
          patch: "hsl(var(--method-patch))",
          delete: "hsl(var(--method-delete))",
          head: "hsl(var(--method-head))",
          options: "hsl(var(--method-options))",
        },
        // Nord frost family members with no existing role token (primary
        // already covers nord8). Currently only the sending spinner.
        frost: {
          teal: "hsl(var(--frost-teal))",
          deep: "hsl(var(--frost-deep))",
        },
        // The WebSocket log's direction colours and the live-connection
        // green (PLAN-WEBSOCKET.md 13f).
        ws: {
          sent: "hsl(var(--ws-sent))",
          received: "hsl(var(--ws-received))",
          ok: "hsl(var(--ws-ok))",
        },
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      keyframes: {
        "nord-reverse-orbit": {
          from: { transform: "rotate(360deg)" },
          to: { transform: "rotate(0deg)" },
        },
        "nord-pulse-glow": {
          "0%, 100%": {
            opacity: "0.4",
            filter: "drop-shadow(0 0 4px hsl(var(--primary)))",
            transform: "scale(0.9)",
          },
          "50%": {
            opacity: "1",
            filter:
              "drop-shadow(0 0 10px hsl(var(--primary))) drop-shadow(0 0 18px hsl(var(--frost-teal)))",
            transform: "scale(1.05)",
          },
        },
      },
      animation: {
        "nord-reverse-orbit": "nord-reverse-orbit 12s linear infinite",
        "nord-pulse-glow": "nord-pulse-glow 2.5s ease-in-out infinite",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
} satisfies Config;
