<template>
  <div class="login-container">
    <div class="login-box">
      <div class="login-header">
        <h1>Kerr</h1>
        <p class="subtitle">Peer-to-peer remote shell</p>
      </div>

      <div class="login-content">
        <p class="login-message">Please sign in to continue</p>

        <button class="google-login-btn" @click="handleGoogleLogin">
          <svg class="google-icon" viewBox="0 0 24 24">
            <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
            <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
            <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
            <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
          </svg>
          Sign in with Google
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const handleGoogleLogin = () => {
  // Get current port from window.location
  const port = window.location.port || '3000';
  // Redirect to login endpoint with port
  window.location.href = `/api/auth/login?port=${port}`;
};
</script>

<style scoped>
.login-container {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #1e1e1e 0%, #2d2d30 100%);
}

.login-box {
  background: #2d2d30;
  border-radius: 8px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
  padding: 40px;
  min-width: 400px;
  text-align: center;
}

.login-header h1 {
  font-size: 48px;
  color: #d4d4d4;
  margin: 0 0 10px 0;
  font-weight: 600;
}

.subtitle {
  color: #858585;
  margin: 0 0 40px 0;
  font-size: 16px;
}

.login-content {
  margin-top: 20px;
}

.login-message {
  color: #d4d4d4;
  margin-bottom: 30px;
  font-size: 14px;
}

.google-login-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  width: 100%;
  padding: 12px 24px;
  background: #ffffff;
  color: #3c4043;
  border: none;
  border-radius: 4px;
  font-size: 16px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.google-login-btn:hover {
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  transform: translateY(-1px);
}

.google-login-btn:active {
  transform: translateY(0);
}

.google-icon {
  width: 20px;
  height: 20px;
}
</style>
