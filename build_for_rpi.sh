#!/bin/bash

# Script de compilación cruzada para Raspberry Pi Zero
echo "Compilando para Raspberry Pi Zero (arm-unknown-linux-gnueabihf)..."

# Verificar que cross esté instalado
if ! command -v cross &> /dev/null; then
    echo "Error: 'cross' no está instalado."
    echo "Instalar con: cargo install cross"
    exit 1
fi

# Limpiar compilaciones anteriores
echo "Limpiando compilaciones anteriores..."
cross clean --target arm-unknown-linux-gnueabihf

# Compilar para RPi Zero
echo "Compilando para arm-unknown-linux-gnueabihf..."
cross build --release --target arm-unknown-linux-gnueabihf

if [ $? -eq 0 ]; then
    echo "¡Compilación exitosa!"
    echo "Ejecutable generado: target/arm-unknown-linux-gnueabihf/release/radio_player"
    
    # Crear directorio de distribución si no existe
    mkdir -p dist
    
    # Copiar ejecutable y ficheros necesarios
    cp target/arm-unknown-linux-gnueabihf/release/radio_player dist/
    cp emisoras.m3u dist/
    cp minutos_noticias.txt dist/
    cp noticias.m3u dist/
    cp run_cvlc.sh dist/
    cp run_on_rpi.sh dist/
    cp check_gpio.sh dist/
    
    # No crear última_estación.txt - el programa lo gestionará automáticamente
    
    echo "Ficheros copiados al directorio 'dist/'"
    echo "Contenido del directorio dist:"
    ls -la dist/
    
    echo ""
# Dar permisos a los scripts y el ejecutable
    chmod +x dist/run_cvlc.sh
    chmod +x dist/run_on_rpi.sh
    chmod +x dist/check_gpio.sh
    chmod +x dist/radio_player
    
    echo "Configuración:"
    echo "  - Botón siguiente: GPIO 20 (pin 38)"
    echo "  - Botón anterior: GPIO 16 (pin 36)"
    echo "  - Botón temporizador: GPIO 12 (pin 32)"
    echo "  - Botón noticias: GPIO 6 (pin 31)"
    echo "  - LCD1602: SDA → GPIO 2 (pin 3), SCL → GPIO 3 (pin 5)"
    echo "  - Todos con pull-up interno"
    echo "  - cvlc ejecutado como usuario normal via wrapper"
    echo "  - Para cambiar los pines, modificar constantes en src/main.rs"
    
else
    echo "Error en la compilación"
    exit 1
fi
