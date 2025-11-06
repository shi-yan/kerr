<template>
  <div class="terminal-container">
    <div class="terminal-header">
      <span class="terminal-title">Remote Shell</span>
      <span v-if="connectionStatus" class="connection-status" :class="connectionStatus">
        {{ connectionStatus }}
      </span>
    </div>
    <div ref="terminalRef" class="terminal"></div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

const terminalRef = ref<HTMLElement | null>(null);
const connectionStatus = ref<'connecting' | 'connected' | 'disconnected' | 'error'>('connecting');

let terminal: Terminal | null = null;
let fitAddon: FitAddon | null = null;
let ws: WebSocket | null = null;

const connectWebSocket = () => {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${window.location.host}/ws/shell`;

  connectionStatus.value = 'connecting';

  ws = new WebSocket(wsUrl);

  ws.onopen = () => {
    connectionStatus.value = 'connected';
    terminal?.writeln('Connected to remote shell...\r\n');
  };

  ws.onmessage = (event) => {
    if (terminal && event.data) {
      terminal.write(event.data);
    }
  };

  ws.onerror = (error) => {
    connectionStatus.value = 'error';
    terminal?.writeln('\r\n\x1b[31mWebSocket error occurred\x1b[0m\r\n');
  };

  ws.onclose = () => {
    connectionStatus.value = 'disconnected';
    terminal?.writeln('\r\n\x1b[33mConnection closed\x1b[0m\r\n');
  };
};

onMounted(() => {
  if (!terminalRef.value) return;

  // Create terminal instance
  terminal = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: 'Consolas, Monaco, "Courier New", monospace',
    theme: {
      background: '#1e1e1e',
      foreground: '#d4d4d4',
      cursor: '#d4d4d4',
      black: '#000000',
      red: '#cd3131',
      green: '#0dbc79',
      yellow: '#e5e510',
      blue: '#2472c8',
      magenta: '#bc3fbc',
      cyan: '#11a8cd',
      white: '#e5e5e5',
      brightBlack: '#666666',
      brightRed: '#f14c4c',
      brightGreen: '#23d18b',
      brightYellow: '#f5f543',
      brightBlue: '#3b8eea',
      brightMagenta: '#d670d6',
      brightCyan: '#29b8db',
      brightWhite: '#e5e5e5',
    },
  });

  // Create fit addon
  fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);

  // Open terminal in DOM
  terminal.open(terminalRef.value);

  // Fit terminal to container
  fitAddon.fit();

  // Handle terminal input
  terminal.onData((data) => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({
        type: 'input',
        data: data,
      }));
    }
  });

  // Handle window resize
  const handleResize = () => {
    if (fitAddon && terminal) {
      fitAddon.fit();
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'resize',
          cols: terminal.cols,
          rows: terminal.rows,
        }));
      }
    }
  };

  window.addEventListener('resize', handleResize);

  // Connect WebSocket
  connectWebSocket();

  // Cleanup
  onBeforeUnmount(() => {
    window.removeEventListener('resize', handleResize);
    if (ws) {
      ws.close();
    }
    if (terminal) {
      terminal.dispose();
    }
  });
});
</script>

<style scoped>
.terminal-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #1e1e1e;
}

.terminal-header {
  padding: 10px 15px;
  background: #2d2d30;
  border-bottom: 1px solid #3e3e42;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.terminal-title {
  font-size: 13px;
  color: #d4d4d4;
  font-weight: 500;
}

.connection-status {
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 3px;
  font-weight: 500;
}

.connection-status.connecting {
  background: #4a4a4a;
  color: #d4d4d4;
}

.connection-status.connected {
  background: #0dbc79;
  color: #ffffff;
}

.connection-status.disconnected {
  background: #666666;
  color: #d4d4d4;
}

.connection-status.error {
  background: #cd3131;
  color: #ffffff;
}

.terminal {
  flex: 1;
  padding: 10px;
  overflow: hidden;
}

:deep(.xterm) {
  height: 100%;
  width: 100%;
}

:deep(.xterm-viewport) {
  overflow-y: auto !important;
}
</style>
