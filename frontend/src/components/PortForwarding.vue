<template>
  <div class="port-forwarding">
    <!-- Button to open the popup -->
    <button class="port-forward-btn" @click="showPopup = true" title="Port Forwarding">
      <span class="material-symbols-outlined">sync_alt</span>
    </button>

    <!-- Popup modal -->
    <div v-if="showPopup" class="modal-overlay" @click.self="showPopup = false">
      <div class="modal-content">
        <div class="modal-header">
          <h3>Port Forwarding</h3>
          <button class="close-btn" @click="showPopup = false">×</button>
        </div>

        <div class="modal-body">
          <!-- Active forwardings list -->
          <div class="forwardings-section">
            <h4>Active Port Forwardings</h4>

            <div v-if="forwardings.length === 0" class="empty-state">
              <span class="material-symbols-outlined">info</span>
              <p>No active port forwardings</p>
            </div>

            <div v-else class="forwardings-list">
              <div
                v-for="forward in forwardings"
                :key="forward.id"
                class="forwarding-item"
              >
                <div class="forwarding-info">
                  <div class="forwarding-name" v-if="forward.name">
                    {{ forward.name }}
                  </div>
                  <div class="forwarding-ports">
                    <span class="port-label">Local:</span>
                    <span class="port-value">{{ forward.localPort }}</span>
                    <span class="arrow">→</span>
                    <span class="port-label">Remote:</span>
                    <span class="port-value">{{ forward.remotePort }}</span>
                  </div>
                  <div class="forwarding-status" :class="forward.status">
                    {{ forward.status }}
                  </div>
                </div>
                <button
                  class="disconnect-forward-btn"
                  @click="disconnectForwarding(forward.id)"
                  title="Stop forwarding"
                >
                  <span class="material-symbols-outlined">close</span>
                </button>
              </div>
            </div>
          </div>

          <!-- Create new forwarding form -->
          <div class="create-section">
            <h4>Create New Port Forwarding</h4>

            <div class="form-group">
              <label for="forward-name">Name (optional)</label>
              <input
                id="forward-name"
                v-model="newForward.name"
                type="text"
                placeholder="e.g., Web Server"
                class="form-input"
              />
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="local-port">Local Port</label>
                <input
                  id="local-port"
                  v-model.number="newForward.localPort"
                  type="number"
                  min="1"
                  max="65535"
                  placeholder="e.g., 8080"
                  class="form-input"
                  required
                />
              </div>

              <div class="form-group">
                <label for="remote-port">Remote Port</label>
                <input
                  id="remote-port"
                  v-model.number="newForward.remotePort"
                  type="number"
                  min="1"
                  max="65535"
                  placeholder="e.g., 3000"
                  class="form-input"
                  required
                />
              </div>
            </div>

            <div v-if="error" class="error-message">
              {{ error }}
            </div>

            <button
              class="forward-btn"
              @click="createForwarding"
              :disabled="!isFormValid || creating"
            >
              {{ creating ? 'Creating...' : 'Start Forwarding' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';

interface PortForwarding {
  id: string;
  name?: string;
  localPort: number;
  remotePort: number;
  status: 'connecting' | 'connected' | 'error';
}

const showPopup = ref(false);
const forwardings = ref<PortForwarding[]>([]);
const creating = ref(false);
const error = ref<string | null>(null);

const newForward = ref({
  name: '',
  localPort: null as number | null,
  remotePort: null as number | null,
});

const isFormValid = computed(() => {
  return (
    newForward.value.localPort !== null &&
    newForward.value.remotePort !== null &&
    newForward.value.localPort > 0 &&
    newForward.value.localPort <= 65535 &&
    newForward.value.remotePort > 0 &&
    newForward.value.remotePort <= 65535
  );
});

const createForwarding = async () => {
  if (!isFormValid.value) return;

  creating.value = true;
  error.value = null;

  try {
    const response = await fetch('/api/port-forward/create', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        name: newForward.value.name || undefined,
        local_port: newForward.value.localPort,
        remote_port: newForward.value.remotePort,
      }),
    });

    if (!response.ok) {
      const data = await response.json();
      throw new Error(data.error || 'Failed to create port forwarding');
    }

    const data = await response.json();

    // Add to list
    forwardings.value.push({
      id: data.id,
      name: newForward.value.name || undefined,
      localPort: newForward.value.localPort!,
      remotePort: newForward.value.remotePort!,
      status: 'connected',
    });

    // Reset form
    newForward.value = {
      name: '',
      localPort: null,
      remotePort: null,
    };
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to create port forwarding';
  } finally {
    creating.value = false;
  }
};

const disconnectForwarding = async (id: string) => {
  try {
    const response = await fetch('/api/port-forward/disconnect', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ id }),
    });

    if (!response.ok) {
      throw new Error('Failed to disconnect port forwarding');
    }

    // Remove from list
    forwardings.value = forwardings.value.filter(f => f.id !== id);
  } catch (e) {
    console.error('Failed to disconnect forwarding:', e);
  }
};
</script>

<style scoped>
.port-forwarding {
  position: relative;
}

.port-forward-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 6px 12px;
  background: transparent;
  border: 1px solid #4fc3f7;
  border-radius: 4px;
  color: #4fc3f7;
  cursor: pointer;
  font-size: 14px;
  transition: all 0.2s;
}

.port-forward-btn:hover {
  background: #4fc3f7;
  color: #1e1e1e;
}

.port-forward-btn .material-symbols-outlined {
  font-size: 18px;
}

.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-content {
  background: #252526;
  border-radius: 8px;
  width: 90%;
  max-width: 600px;
  max-height: 80vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px;
  border-bottom: 1px solid #3e3e42;
}

.modal-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 500;
  color: #d4d4d4;
}

.close-btn {
  background: transparent;
  border: none;
  color: #858585;
  font-size: 32px;
  line-height: 1;
  cursor: pointer;
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: all 0.2s;
}

.close-btn:hover {
  background: #3e3e42;
  color: #d4d4d4;
}

.modal-body {
  padding: 20px;
  overflow-y: auto;
  flex: 1;
}

.forwardings-section,
.create-section {
  margin-bottom: 30px;
}

.forwardings-section h4,
.create-section h4 {
  margin: 0 0 15px 0;
  font-size: 14px;
  font-weight: 500;
  color: #858585;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 30px 20px;
  color: #858585;
  background: #2d2d30;
  border-radius: 6px;
}

.empty-state .material-symbols-outlined {
  font-size: 40px;
  margin-bottom: 10px;
  opacity: 0.5;
}

.forwardings-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.forwarding-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 15px;
  background: #2d2d30;
  border-radius: 6px;
  border: 1px solid #3e3e42;
}

.forwarding-info {
  flex: 1;
}

.forwarding-name {
  font-size: 14px;
  font-weight: 500;
  color: #d4d4d4;
  margin-bottom: 8px;
}

.forwarding-ports {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #858585;
  margin-bottom: 6px;
}

.port-label {
  color: #858585;
}

.port-value {
  color: #4fc3f7;
  font-weight: 500;
}

.arrow {
  color: #858585;
}

.forwarding-status {
  font-size: 12px;
  padding: 2px 8px;
  border-radius: 3px;
  display: inline-block;
}

.forwarding-status.connecting {
  background: #5a5a1d;
  color: #f4e771;
}

.forwarding-status.connected {
  background: #1d5a2f;
  color: #71f48f;
}

.forwarding-status.error {
  background: #5a1d1d;
  color: #f48771;
}

.disconnect-forward-btn {
  background: transparent;
  border: 1px solid #f48771;
  color: #f48771;
  padding: 6px 10px;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}

.disconnect-forward-btn:hover {
  background: #f48771;
  color: #1e1e1e;
}

.disconnect-forward-btn .material-symbols-outlined {
  font-size: 18px;
}

.form-group {
  margin-bottom: 15px;
  flex: 1;
}

.form-row {
  display: flex;
  gap: 15px;
}

.form-group label {
  display: block;
  font-size: 13px;
  color: #858585;
  margin-bottom: 6px;
  font-weight: 500;
}

.form-input {
  width: 100%;
  padding: 10px 12px;
  background: #2d2d30;
  border: 1px solid #3e3e42;
  border-radius: 4px;
  color: #d4d4d4;
  font-size: 14px;
  transition: border-color 0.2s;
}

.form-input:focus {
  outline: none;
  border-color: #4fc3f7;
}

.form-input::placeholder {
  color: #5a5a5a;
}

.error-message {
  padding: 10px 12px;
  background: #5a1d1d;
  color: #f48771;
  border-radius: 4px;
  font-size: 13px;
  margin-bottom: 15px;
}

.forward-btn {
  width: 100%;
  padding: 12px;
  background: #4fc3f7;
  color: #1e1e1e;
  border: none;
  border-radius: 4px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.forward-btn:hover:not(:disabled) {
  background: #6dd4ff;
}

.forward-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
