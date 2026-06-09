# Supp - Multi-Session WhatsApp Manager

Aplicación de escritorio para gestionar múltiples sesiones de WhatsApp Web simultáneamente.

## Características

- 🚀 Múltiples sesiones de WhatsApp Web aisladas
- 🎨 Interfaz moderna con tema oscuro y acentos verde eléctrico
- 🔔 Indicadores de mensajes no leídos
- ⚙️ Configuración del sistema (inicio con Windows, bandeja del sistema)
- 💾 Persistencia de sesiones y configuraciones

## Desarrollo

```bash
# Instalar dependencias
npm install

# Ejecutar en modo desarrollo (Electron + Vite)
npm run electron:dev
```

## Compilación

```bash
# Compilar para Windows
npm run electron:build:win

# Compilar para macOS
npm run electron:build:mac

# Compilar para Linux
npm run electron:build:linux
```

## Estructura del Proyecto

```
Supp/
├── electron/
│   ├── main.js       # Proceso principal de Electron
│   └── preload.js    # Script de preload (puente IPC)
├── src/
│   ├── App.jsx       # Componente principal de React
│   ├── index.css     # Estilos globales con Tailwind
│   └── main.jsx      # Punto de entrada de React
├── public/
│   └── icon.png      # Icono de la aplicación
└── package.json      # Configuración del proyecto
```

## Tecnologías

- **Electron** - Framework de aplicaciones de escritorio
- **React** - Biblioteca de interfaz de usuario
- **Tailwind CSS** - Framework de estilos
- **Vite** - Herramienta de compilación
- **Lucide React** - Iconos

## Autor

Javier Medina

## Licencia

MIT
