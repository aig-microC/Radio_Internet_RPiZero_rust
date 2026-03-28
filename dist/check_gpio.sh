#!/bin/bash

# Script para verificar la configuración de GPIO
echo "Verificación de GPIO para el reproductor de radio"
echo "==============================================="
echo ""

# Verificar si estamos en una Raspberry Pi
if [ ! -f /proc/device-tree/model ]; then
    echo "ERROR: No se detecta una Raspberry Pi"
    echo "Este programa solo funciona en hardware real"
    exit 1
fi

echo "Modelo de Raspberry Pi detectado:"
cat /proc/device-tree/model
echo ""

# Verificar acceso a GPIO
if [ ! -d /sys/class/gpio ]; then
    echo "ERROR: No se encuentra el directorio /sys/class/gpio"
    echo "Ejecute: sudo raspi-config y habilite GPIO"
    exit 1
fi

echo "GPIO disponible: ✓"
echo ""

# Verificar permisos
if [ ! -w /sys/class/gpio ]; then
    echo "ADVERTENCIA: No tiene permisos de escritura en /sys/class/gpio"
    echo "El programa puede funcionar igualmente"
    echo ""
else
    echo "Permisos de GPIO: ✓"
    echo ""
fi

# Verificar si el programa está compilado para ARM
if [ -f "./radio_player" ]; then
    echo "Ejecutable encontrado: ✓"
    file ./radio_player
    echo ""
else
    echo "ERROR: No se encuentra el ejecutable radio_player"
    echo "Compile con: ./build_for_rpi.sh"
    exit 1
fi

# Verificar cvlc
if command -v cvlc &> /dev/null; then
    echo "VLC/cvlc disponible: ✓"
else
    echo "ERROR: cvlc no está instalado"
    echo "Instale con: sudo apt-get install vlc"
    exit 1
fi

echo ""
echo "✓ Todo parece correcto para ejecutar el programa"
echo ""
echo "Para ejecutar:"
echo "  ./run_on_rpi.sh"
echo ""
echo "Control de emisoras:"
echo "  - Botón SIGUIENTE (GPIO 20): siguiente emisora"
echo "  - Botón ANTERIOR (GPIO 16): emisora anterior"
echo "  - Botón TEMPORIZADOR (GPIO 12): temporización de apagado (90→80→70...→0 min)"
echo "    Pulsación larga (>4s): apagado inmediato"
echo ""
echo "Conexión de los botones:"
echo "  GPIO 20 (pin 38) ────┐    GPIO 16 (pin 36) ────┐    GPIO 12 (pin 32) ────┐"
echo "                       │                           │                           │"
echo "                    ┌───┐                     ┌───┐                     ┌───┐"
echo "                    │ SIG │                     │ ANT │                     │ TIM │"
echo "                    └───┘                     └───┘                     └───┘"
echo "                       │                           │                           │"
echo "                      GND                         GND                         GND"