#!/bin/bash

# Script para ejecutar cvlc como usuario normal desde root
# Uso: ./run_cvlc.sh "URL"
URL="$1"

if [ -z "$URL" ]; then
    echo "Error: Se necesita una URL como parámetro"
    exit 1
fi

# Buscar un usuario normal para ejecutar cvlc
for user in pi $(ls /home | grep -v root); do
    if [ "$user" != "root" ] && [ -d "/home/$user" ]; then
        # Ejecutar cvlc como usuario normal en foreground (sin &)
        exec sudo -u "$user" cvlc --no-video --no-interact --quiet "$URL"
    fi
done

echo "Error: No se encontró un usuario normal para ejecutar cvlc"
exit 1