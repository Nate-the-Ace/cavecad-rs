// Serves the live-editing relay on the dev and preview servers at /sync, so one command
// (`npm run dev`) is all a second device needs: open the laptop's address in its browser.
import { attachRelay, createRelay } from './sync-relay.mjs';

export default function caveSync() {
  const relay = createRelay();
  return {
    name: 'cave-sync',
    configureServer(server) {
      if (server.httpServer) attachRelay(server.httpServer, relay);
    },
    configurePreviewServer(server) {
      if (server.httpServer) attachRelay(server.httpServer, relay);
    },
  };
}
