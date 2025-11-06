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

      <div v-else-if="connections.length === 0" class="empty-state">
        <div class="empty-icon">📡</div>
        <p>No registered connections found</p>
        <p class="empty-hint">Register a connection using: <code>kerr serve --register &lt;alias&gt;</code></p>
      </div>

      <div v-else class="connections-list">
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
          <div class="connection-arrow">
            <span v-if="connectingTo === connection.connection_string">⏳</span>
            <span v-else>→</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import type { Connection } from '../types/connection';

const emit = defineEmits<{
  connected: [];
}>();

const connections = ref<Connection[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const connectingTo = ref<string | null>(null);

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
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load connections';
  } finally {
    loading.value = false;
  }
};

const selectConnection = async (connection: Connection) => {
  if (connectingTo.value) return; // Prevent multiple clicks

  connectingTo.value = connection.connection_string;

  try {
    const response = await fetch('/api/connection/connect', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        connection_string: connection.connection_string,
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

.connection-arrow {
  font-size: 24px;
  color: #858585;
  margin-left: 15px;
}
</style>
