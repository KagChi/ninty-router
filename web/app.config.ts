import { defineConfig } from "@solidjs/start/config";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  vite: {
    plugins: [tailwindcss()],
    server: {
      proxy: {
        "/api": "http://localhost:20128",
        "/v1": "http://localhost:20128",
      },
    },
  },
  server: {
    preset: "static",
    prerender: {
      routes: ["/", "/login"],
      crawlLinks: false,
    },
  },
});
