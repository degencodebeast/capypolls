import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { XionProvider } from "@/library/providers/xion/provider";
import "./globals.css";

const inter = Inter({ subsets: ["latin"] });

export const metadata: Metadata = {
  title: "CapyPolls - No Loss Prediction Markets",
  description: "The first no-loss prediction market with memecoins",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className={inter.className}>
        <XionProvider>{children}</XionProvider>
      </body>
    </html>
  );
}
