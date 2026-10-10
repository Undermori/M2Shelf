import { defineConfig } from "vitest/config";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

// Only local CMaps/decoders and the dependency license; never redistribute font files.
function pdfResources():Plugin {
 const root=resolve('node_modules/pdfjs-dist');
 const files=new Map<string,string>();
 for(const dir of ['cmaps','wasm'])for(const name of readdirSync(resolve(root,dir)))if(name.endsWith('.bcmap')||name.endsWith('.wasm'))files.set(`pdf-resources/${dir}/${name}`,resolve(root,dir,name));
 files.set('pdf-resources/LICENSE',resolve(root,'LICENSE'));
 return {name:'m2shelf-local-pdf-resources',generateBundle(){for(const [fileName,path] of Array.from(files))this.emitFile({type:'asset',fileName,source:readFileSync(path)});},configureServer(server){server.middlewares.use((request,response,next)=>{const key=request.url?.split('?')[0].slice(1);const path=key&&files.get(key);if(!path){next();return;}response.setHeader('Content-Type',path.endsWith('.wasm')?'application/wasm':'application/octet-stream');response.end(readFileSync(path));});}};
}

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  test: { include: ["src/**/*.test.{ts,tsx}"] },
  plugins: [react(),pdfResources()],
  // Scan only the real app entry. Archived test HTML in .tmp must not enter dev prebundling.
  optimizeDeps: { entries: ["index.html"] },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host ?? "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Packaging creates/removes executable files which Windows may temporarily lock.
      // These generated files never contribute to frontend hot reload.
      ignored: ["**/src-tauri/**", "**/bundle/**", "**/.tmp/**", "**/.tools/**", "**/output/playwright/**"],
    },
  },
});
