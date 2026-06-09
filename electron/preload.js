const { contextBridge, ipcRenderer } = require('electron');

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld('electron', {
    // Toggle startup with Windows
    toggleStartup: async (enable) => {
        try {
            return await ipcRenderer.invoke('toggle-startup', enable);
        } catch (error) {
            console.error('Error toggling startup:', error);
            return { success: false, error: error.message };
        }
    },

    // Set close to tray behavior
    setCloseToTray: async (enable) => {
        try {
            return await ipcRenderer.invoke('set-close-to-tray', enable);
        } catch (error) {
            console.error('Error setting close to tray:', error);
            return { success: false, error: error.message };
        }
    },

    // Minimize to tray
    minimizeToTray: async () => {
        try {
            return await ipcRenderer.invoke('minimize-to-tray');
        } catch (error) {
            console.error('Error minimizing to tray:', error);
            return { success: false, error: error.message };
        }
    },

    // Quit application
    quitApp: async () => {
        try {
            return await ipcRenderer.invoke('quit-app');
        } catch (error) {
            console.error('Error quitting app:', error);
            return { success: false, error: error.message };
        }
    },

    // Get platform info
    platform: process.platform,

    // Check if running in Electron
    isElectron: true
});

// Log that preload script has loaded
console.log('Preload script loaded successfully');
