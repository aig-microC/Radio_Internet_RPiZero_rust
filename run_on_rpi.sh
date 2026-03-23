#!/bin/bash

# Script para ejecutar el reproductor en la RPi Zero con manejo correcto de permisos
echo "Reproductor de Radio para RPi Zero con control GPIO cuádruple y LCD1602"
echo "========================================================================"
echo ""

# Verificar que cvlc esté instalado
if ! command -v cvlc &> /dev/null; then
    echo "Error: cvlc no está instalado."
    echo "Instalar con: sudo apt-get install vlc"
    exit 1
fi

# Verificar ficheros necesarios
if [ ! -f "emisoras.m3u" ]; then
    echo "Error: No se encuentra el fichero emisoras.m3u"
    exit 1
fi

# Determinar el usuario actual
CURRENT_USER=$(whoami)
echo "Usuario actual: $CURRENT_USER"

echo "Configuración:"
echo "  - Botón siguiente: GPIO 20 (pin 38)"
echo "  - Botón anterior: GPIO 16 (pin 36)"
echo "  - Botón temporizador: GPIO 12 (pin 32)"
echo "  - Botón noticias: GPIO 6 (pin 31)"
echo "  - LCD1602: SDA → GPIO 2 (pin 3), SCL → GPIO 3 (pin 5)"
echo "  - Modo: pull-up interno en todos los botones"
echo "  - Control: conectar brevemente a masa"
echo "  - Salida: CTRL+C para guardar y salir"
echo ""

# Ejecutar el programa directamente
echo "Ejecutando programa con control GPIO cuádruple y display LCD1602..."
echo "Presione CTRL+C para salir"
echo ""

# Ejecutar el programa
./radio_player