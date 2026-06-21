/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        dark: {
          100: "#1e1e2e",
          200: "#181825",
          300: "#11111b",
          400: "#313244",
        },
        accent: {
          cyan: "#89dceb",
          green: "#a6e3a1",
          red: "#f38ba8",
          yellow: "#f9e2af",
          purple: "#cba6f7",
        },
      },
    },
  },
  plugins: [],
};
