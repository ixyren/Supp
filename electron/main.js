const { app, BrowserWindow, ipcMain, Tray, Menu, MenuItem, session, nativeImage } = require('electron');
const path = require('path');
const fs = require('fs');

// 1. VARIABLES GLOBALES (Evita Garbage Collection)
let mainWindow = null;
let tray = null;
let isQuitting = false;

// 2. FUNCIÓN DE ICONO SEGURA (Blindaje)
function getIconPath() {
    try {
        // Intentar buscar en la carpeta public relativa al script
        // En producción (ASAR), esto busca dentro del paquete si 'public' está incluido en 'files'
        const currentDir = __dirname;
        const possiblePath = path.join(currentDir, '../public/icon.ico');
        return possiblePath;
    } catch (e) {
        console.error('Error calculando ruta de icono:', e);
        return null; // Retornar null no rompe la app, solo sale sin icono
    }
}

// 5. LÓGICA PORTABLE (Mantener)
if (process.env.PORTABLE_EXECUTABLE_DIR) {
    const portableDataPath = path.join(process.env.PORTABLE_EXECUTABLE_DIR, 'Supp_Data');
    app.setPath('userData', portableDataPath);
    app.setPath('sessionData', path.join(portableDataPath, 'sessions'));
}

const configPath = path.join(app.getPath('userData'), 'config.json');

// Default settings
const defaultSettings = {
    closeToTray: true,
    startMinimized: false,
    openAtLogin: false
};

let currentSettings = { ...defaultSettings };

// Helper functions for settings
function loadSettings() {
    try {
        if (fs.existsSync(configPath)) {
            const data = fs.readFileSync(configPath, 'utf-8');
            return { ...defaultSettings, ...JSON.parse(data) };
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
    return defaultSettings;
}

function saveSettings(settings) {
    try {
        fs.writeFileSync(configPath, JSON.stringify(settings, null, 2));
    } catch (error) {
        console.error('Error saving settings:', error);
    }
}

// Load settings on startup
currentSettings = loadSettings();

function createWindow() {
    const iconPath = getIconPath();

    // 3. OPTIMIZACIÓN DE ARRANQUE
    mainWindow = new BrowserWindow({
        width: 1400,
        height: 900,
        minWidth: 1000,
        minHeight: 600,
        backgroundColor: '#0a0a0a', // Evita pantalla blanca
        icon: iconPath || undefined, // Si es null, usa el default del sistema
        webPreferences: {
            webviewTag: true,
            nodeIntegration: false,
            contextIsolation: true,
            preload: path.join(__dirname, 'preload.js')
        },
        frame: true,
        titleBarStyle: 'default',
        show: false // Oculto inicialmente
    });

    mainWindow.setMenuBarVisibility(false);

    const isDev = !app.isPackaged;
    if (isDev) {
        mainWindow.loadURL('http://localhost:5173');
    } else {
        mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
    }

    // Mostrar solo cuando esté listo (Speed Up perception)
    mainWindow.once('ready-to-show', () => {
        const startMinimized = app.getLoginItemSettings().wasOpenedAsHidden || currentSettings.startMinimized;
        if (!startMinimized) {
            mainWindow.show();
        }
    });

    // 5. LÓGICA DE CIERRE (Evitar Zombies)
    mainWindow.on('close', (event) => {
        if (currentSettings.closeToTray && !isQuitting) {
            event.preventDefault();
            mainWindow.hide();
        }
        // Si no, deja cerrar (que destruirá la ventana)
    });

    // Permissions
    session.defaultSession.setPermissionRequestHandler((webContents, permission, callback) => {
        const allowed = ['media', 'microphone', 'camera', 'notifications'];
        if (allowed.includes(permission)) {
            callback(true);
        } else {
            callback(false);
        }
    });

    // WhatsApp Popups
    mainWindow.webContents.on('did-attach-webview', (event, webviewWebContents) => {
        webviewWebContents.setWindowOpenHandler(() => ({ action: 'allow' }));
    });
}

// 4. CREACIÓN ROBUSTA DEL TRAY
function createTray() {
    if (tray) {
        try {
            tray.destroy(); // Evitar duplicados
        } catch (e) {
            console.error('Error destruyendo tray anterior:', e);
        }
    }

    const iconPath = getIconPath();

    // Si no hay icono válido, NO creamos el Tray para evitar crash
    if (!iconPath) {
        console.warn('No se encontró icono válido, Tray deshabilitado.');
        return;
    }

    try {
        tray = new Tray(iconPath);
        tray.setToolTip('Supp - Multi WhatsApp Manager');

        const contextMenu = Menu.buildFromTemplate([
            { label: 'Abrir Supp', click: () => mainWindow.show() },
            { type: 'separator' },
            {
                label: 'Salir',
                click: () => {
                    isQuitting = true;
                    app.quit();
                }
            }
        ]);

        tray.setContextMenu(contextMenu);

        tray.on('double-click', () => {
            if (mainWindow && mainWindow.isVisible()) {
                mainWindow.hide();
            } else if (mainWindow) {
                mainWindow.show();
            }
        });
    } catch (error) {
        console.error('Error creando Tray:', error);
        // La app sigue funcionando sin Tray
    }
}

// IPC Handlers (Mantener lógica existente)
ipcMain.handle('get-settings', () => currentSettings);

ipcMain.handle('set-setting', (event, key, value) => {
    currentSettings[key] = value;
    saveSettings(currentSettings);
    return { success: true };
});

ipcMain.handle('toggle-startup', async (event, enable) => {
    app.setLoginItemSettings({ openAtLogin: enable, openAsHidden: false, path: app.getPath('exe') });
    currentSettings.openAtLogin = enable;
    saveSettings(currentSettings);
    return { success: true };
});

ipcMain.handle('set-close-to-tray', async (event, enable) => {
    currentSettings.closeToTray = enable;
    saveSettings(currentSettings);
    return { success: true };
});

ipcMain.handle('minimize-to-tray', () => {
    if (mainWindow) mainWindow.hide();
    return { success: true };
});

ipcMain.handle('quit-app', () => {
    isQuitting = true;
    app.quit();
    return { success: true };
});

// SINGLE INSTANCE LOCK
const gotTheLock = app.requestSingleInstanceLock();

if (!gotTheLock) {
    // Si no conseguimos el bloqueo, significa que ya hay otra instancia. Cerrar esta.
    app.quit();
} else {
    // Si somos la instancia principal, escuchamos si alguien intenta abrir otra
    app.on('second-instance', (event, commandLine, workingDirectory) => {
        // Alguien intentó ejecutar una segunda instancia, enfocamos nuestra ventana
        if (mainWindow) {
            // Si está minimizada, restaurar
            if (mainWindow.isMinimized()) mainWindow.restore();
            // Si está oculta (en bandeja), mostrar
            if (!mainWindow.isVisible()) mainWindow.show();
            // Enfocar
            mainWindow.focus();
        }
    });

    // App Lifecycle
    app.whenReady().then(() => {
        createWindow();
        createTray();

        // 6. PERSISTENCIA Y MENÚ CONTEXTUAL (Mantener)
        app.on('web-contents-created', (event, contents) => {
            contents.on('context-menu', (e, props) => {
                const { mediaType, srcURL, x, y } = props;
                const menu = new Menu();

                if (mediaType === 'image') {
                    menu.append(new MenuItem({
                        label: 'Guardar imagen como...',
                        click: () => contents.downloadURL(srcURL)
                    }));
                    menu.append(new MenuItem({
                        label: 'Copiar imagen',
                        click: () => contents.copyImageAt(x, y)
                    }));
                }

                if (mediaType === 'video' || mediaType === 'audio') {
                    menu.append(new MenuItem({
                        label: 'Guardar archivo como...',
                        click: () => contents.downloadURL(srcURL)
                    }));
                }

                if (props.isEditable || props.selectionText.length > 0) {
                    menu.append(new MenuItem({ role: 'copy', label: 'Copiar' }));
                    menu.append(new MenuItem({ role: 'cut', label: 'Cortar' }));
                    menu.append(new MenuItem({ role: 'paste', label: 'Pegar' }));
                }

                if (menu.items.length > 0) {
                    menu.popup({ window: mainWindow, x, y });
                }
            });

            // Gestión de Descargas
            contents.session.on('will-download', (event, item, webContents) => {
                const originalName = item.getFilename();
                const ext = path.extname(originalName);
                const baseName = path.basename(originalName, ext);
                const uniqueSuffix = `${Date.now()}-${Math.floor(Math.random() * 1000)}`;
                const newFileName = `${baseName}_${uniqueSuffix}${ext}`;

                item.setSaveDialogOptions({
                    title: 'Guardar archivo multimedia',
                    defaultPath: newFileName,
                    buttonLabel: 'Guardar'
                });

                item.on('done', (event, state) => {
                    if (state === 'completed') {
                        console.log('Descarga completada:', newFileName);
                    }
                });
            });
        });

        app.on('activate', () => {
            if (BrowserWindow.getAllWindows().length === 0) {
                createWindow();
            } else {
                if (mainWindow) mainWindow.show();
            }
        });
    });

    app.on('window-all-closed', () => {
        if (process.platform !== 'darwin') {
            if (isQuitting) {
                app.quit();
            }
        }
    });

    app.on('before-quit', () => {
        isQuitting = true;
    });
}
