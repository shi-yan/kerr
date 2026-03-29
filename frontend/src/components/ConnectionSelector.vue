<template>
  <div class="connection-selector">
    <div class="selector-container">
      <div class="selector-header">
        <h2>🌌 Select a Connection</h2>
        <p class="subtitle">Choose a registered connection to connect to</p>
      </div>

      <div v-if="loading" class="loading">
        <div class="spinner"></div>
        <p>Loading connections...</p>
      </div>

      <div v-else-if="error" class="error">
        <p>{{ error }}</p>
        <button @click="loadConnections" class="retry-button">Retry</button>
      </div>

      <div v-if="fromCache" class="cache-banner">
        Offline mode — showing cached connections (AWS registry unreachable)
      </div>

      <div v-if="!loading && !error && connections.length === 0" class="empty-state">
        <div class="empty-icon">📡</div>
        <p>No registered connections found</p>
        <p class="empty-hint">Register a connection using: <code>kerr serve --register &lt;alias&gt;</code></p>
      </div>

      <div v-if="!loading && !error && connections.length > 0" class="connections-list">
        <div
          v-for="connection in connections"
          :key="connection.connection_string"
          class="connection-item"
          :class="{ connecting: connectingTo === connection.connection_string }"
          @click="selectConnection(connection)"
        >
          <div class="connection-info">
            <div class="connection-alias">
              {{ connection.alias || 'Unnamed Connection' }}
            </div>
            <div class="connection-host">
              {{ connection.host_name }}
            </div>
            <div class="connection-date">
              Registered: {{ formatDate(connection.registered_at) }}
            </div>
          </div>
          <div class="connection-actions">
            <button
              class="qr-button"
              title="Show QR code for iOS"
              @click.stop="openQR(connection)"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/>
                <rect x="5" y="5" width="3" height="3" fill="currentColor" stroke="none"/><rect x="16" y="5" width="3" height="3" fill="currentColor" stroke="none"/><rect x="5" y="16" width="3" height="3" fill="currentColor" stroke="none"/>
                <line x1="14" y1="14" x2="14" y2="14"/><line x1="17" y1="14" x2="17" y2="14"/><line x1="20" y1="14" x2="20" y2="14"/><line x1="14" y1="17" x2="14" y2="17"/><line x1="17" y1="17" x2="20" y2="17"/><line x1="20" y1="20" x2="14" y2="20"/><line x1="17" y1="20" x2="17" y2="20"/>
              </svg>
            </button>
            <span v-if="connectingTo === connection.connection_string">⏳</span>
            <span v-else class="arrow">→</span>
          </div>
        </div>
      </div>
    </div>

    <!-- QR modal -->
    <div v-if="qrConnection" class="qr-overlay" @click.self="closeQR">
      <div class="qr-modal">
        <div class="qr-modal-header">
          <div class="qr-modal-title">{{ qrConnection.alias || 'Unnamed Connection' }}</div>
          <div class="qr-modal-host">{{ qrConnection.host_name }}</div>
        </div>
        <div class="qr-canvas-wrap">
          <canvas ref="qrCanvas"></canvas>
        </div>
        <p class="qr-hint">Scan with Kerr iOS app to add this connection</p>
        <button class="qr-close-btn" @click="closeQR">Close</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch, nextTick } from 'vue';
import QRCode from 'qrcode';
import type { Connection } from '../types/connection';

const emit = defineEmits<{
  connected: [];
}>();

const connections = ref<Connection[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const connectingTo = ref<string | null>(null);
const fromCache = ref(false);

const qrConnection = ref<Connection | null>(null);
const qrCanvas = ref<HTMLCanvasElement | null>(null);

const loadConnections = async () => {
  loading.value = true;
  error.value = null;

  try {
    const response = await fetch('/api/connection/list');
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    const data = await response.json();
    connections.value = data.connections;
    fromCache.value = data.from_cache ?? false;
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load connections';
  } finally {
    loading.value = false;
  }
};

const selectConnection = async (connection: Connection) => {
  if (connectingTo.value) return;

  connectingTo.value = connection.connection_string;

  try {
    const response = await fetch('/api/connection/connect', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        connection_string: connection.connection_string,
        alias: connection.alias,
      }),
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const data = await response.json();
    if (data.success) {
      emit('connected');
    } else {
      error.value = data.message;
      connectingTo.value = null;
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to connect';
    connectingTo.value = null;
  }
};

const openQR = (connection: Connection) => {
  qrConnection.value = connection;
};

const closeQR = () => {
  qrConnection.value = null;
};

watch(qrConnection, async (conn) => {
  if (!conn) return;
  await nextTick();
  if (!qrCanvas.value) return;
  const payload = JSON.stringify({
    alias: conn.alias ?? '',
    host_name: conn.host_name,
    cs: conn.connection_string,
  });
  await QRCode.toCanvas(qrCanvas.value, payload, {
    width: 280,
    margin: 2,
    color: { dark: '#000000', light: '#ffffff' },
  });
});

const formatDate = (timestamp: number): string => {
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString() + ' ' + date.toLocaleTimeString();
};

onMounted(() => {
  loadConnections();
});
</script>

<style scoped>
.connection-selector {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  background: #1e1e1e;
  padding: 20px;
}

.selector-container {
  width: 100%;
  max-width: 600px;
  background: #252526;
  border-radius: 8px;
  padding: 30px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
}

.selector-header {
  text-align: center;
  margin-bottom: 30px;
}

.selector-header h2 {
  font-size: 24px;
  font-weight: 500;
  color: #d4d4d4;
  margin-bottom: 10px;
}

.subtitle {
  font-size: 14px;
  color: #858585;
}

.loading {
  text-align: center;
  padding: 40px 20px;
  color: #858585;
}

.spinner {
  width: 40px;
  height: 40px;
  margin: 0 auto 15px;
  border: 3px solid #3e3e42;
  border-top-color: #4fc3f7;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.error {
  text-align: center;
  padding: 20px;
  color: #f48771;
  background: #5a1d1d;
  border-radius: 4px;
  margin-bottom: 20px;
}

.retry-button {
  margin-top: 15px;
  padding: 8px 16px;
  background: #4fc3f7;
  color: #1e1e1e;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
}

.retry-button:hover {
  background: #6dd4ff;
}

.cache-banner {
  background: #3d2e00;
  border: 1px solid #7a5c00;
  color: #e0b84a;
  border-radius: 4px;
  padding: 10px 14px;
  font-size: 13px;
  margin-bottom: 16px;
  text-align: center;
}

.empty-state {
  text-align: center;
  padding: 40px 20px;
  color: #858585;
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 15px;
}

.empty-hint {
  margin-top: 10px;
  font-size: 13px;
}

.empty-hint code {
  background: #1e1e1e;
  padding: 2px 6px;
  border-radius: 3px;
  font-family: 'Consolas', 'Monaco', monospace;
}

.connections-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.connection-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  background: #2d2d30;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
  border: 2px solid transparent;
}

.connection-item:hover {
  background: #333336;
  border-color: #4fc3f7;
}

.connection-item.connecting {
  background: #333336;
  cursor: wait;
  opacity: 0.7;
}

.connection-info {
  flex: 1;
}

.connection-alias {
  font-size: 16px;
  font-weight: 500;
  color: #d4d4d4;
  margin-bottom: 4px;
}

.connection-host {
  font-size: 13px;
  color: #4fc3f7;
  margin-bottom: 4px;
}

.connection-date {
  font-size: 12px;
  color: #858585;
}

.connection-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-left: 15px;
}

.qr-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  background: #3e3e42;
  border: 1px solid #555;
  border-radius: 6px;
  color: #858585;
  cursor: pointer;
  transition: all 0.15s;
  flex-shrink: 0;
}

.qr-button:hover {
  background: #4fc3f7;
  border-color: #4fc3f7;
  color: #1e1e1e;
}

.arrow {
  font-size: 24px;
  color: #858585;
}

/* QR Modal */
.qr-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.qr-modal {
  background: #252526;
  border-radius: 10px;
  padding: 28px 32px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  min-width: 320px;
}

.qr-modal-header {
  text-align: center;
}

.qr-modal-title {
  font-size: 18px;
  font-weight: 600;
  color: #d4d4d4;
  margin-bottom: 4px;
}

.qr-modal-host {
  font-size: 13px;
  color: #4fc3f7;
}

.qr-canvas-wrap {
  background: #ffffff;
  border-radius: 6px;
  padding: 8px;
  line-height: 0;
}

.qr-hint {
  font-size: 12px;
  color: #858585;
  text-align: center;
}

.qr-close-btn {
  margin-top: 4px;
  padding: 8px 24px;
  background: #3e3e42;
  color: #d4d4d4;
  border: 1px solid #555;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  transition: background 0.15s;
}

.qr-close-btn:hover {
  background: #505054;
}
</style>
