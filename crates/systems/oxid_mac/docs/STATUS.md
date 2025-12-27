# Estado del Emulador Macintosh Plus (Oxide-Mac)

## Resumen Ejecutivo
El emulador ha avanzado significativamente, logrando la ejecución de la secuencia de arranque (Boot Sequence) del ROM de Macintosh Plus (v3). Hemos superado el bloqueo inicial del "Sad Mac" instantáneo mediante la corrección de la configuración de memoria y el control del overlay del VIA. Actualmente, el emulador ejecuta millones de instrucciones y muestra actividad en VRAM, pero finalmente cae en un error de **Sad Mac 03FFFF** (Test de Bus de Datos fallido) o un bloqueo en tiempo de ejecución.

## Logros y Mejoras Implementadas

### 1. Sistema de Memoria Robusto
- **Refactorización a `memory.rs`**: Se aisló la lógica de la RAM en un componente `MacRam` dedicado.
- **Manejo de Mirroring**: Se implementó una lógica precisa que deshabilita el "mirroring" (espejo) para configuraciones de 4MB (evitando corrupción de memoria baja por escrituras altas) y lo mantiene para 128K/512K.
- **Mapa de Memoria Denso**: En `bus.rs`, se definieron regiones precisas para ROM (`0x40xxxx`), SCSI (`0x58xxxx`, separado para evitar conflictos), SCC y IWM.
- **Configuración por Defecto**: Se estableció 4MB de RAM para el Mac Plus para minimizar problemas de hardware legacy.

### 2. Emulación del VIA (Versatile Interface Adapter)
- **Shift Register (Teclado)**: Se implementó la emulación del registro de desplazamiento.
    - **Interrupciones**: Generación inmediata de interrupciones tras escritura/lectura para simular "Transferencia Completa", vital para que la ROM no se cuelgue esperando al hardware.
    - **Protocolo Inquiry**: El VIA ahora responde al comando de identidad (`0x10`) devolviendo el ID del teclado Mac Plus (`0x0B`), satisfaciendo la validación de periféricos del POST.
- **Timers y Flags**: Se mejoró la precisión de los Timers 1 y 2 y el manejo de flags en el IFR (Interrupt Flag Register) usando `Cell` para permitir "Clear on Read" sin violar inmutabilidad.

## El Problema Persistente: Sad Mac 03FFFF

A pesar de las mejoras, el error **03FFFF** persiste o reaparece en etapas tardías del arranque.

*   **Código**: `03` = Failure in Memory Test.
*   **Subcódigo**: `FFFF` = Data Bus Test Failure (Escritura/Lectura falló en todos los bancos o líneas de datos atrapadas).

### Causas Descartadas (Fixes aplicados)
1.  **Mirroring de RAM**: Se sospechaba que escribir en direcciones altas corrompía el vector de reset (`0x0`). **Estado**: Solucionado con 4MB RAM y lógica `memory.rs`.
2.  **Falta de Teclado**: Se sospechaba que el sistema crasheaba por timeout esperando el teclado. **Estado**: Implementado handshake correcto en el VIA. El sistema bootea más lejos, pero falla eventualmente.
3.  **Overlay del ROM**: Confirmado que se desactiva correctamente (instrucción 158).

### Hipótesis Actuales (Investigación Futura)

Si la memoria está "perfecta" y el VIA responde, ¿por qué falla el test de bus de datos?

1.  **Instrucciones de CPU (`oxid68k`)**:
    *   **CRÍTICO**: Se descubrió que el crate `oxid68k` tiene **0 tests unitarios**.
    *   El trace del crash muestra un bucle de cálculo (`0x40016A`..`0x40019E`) que usa instrucciones `ROL`/`ASL`, `LSR`, `ADD`. Al terminar, salta al manejador de error `0x4001AC` con código `D0=0003FFFF`.
    *   **Sospecha**: Bug en la implementación de instrucciones de rotación/shift o flags en `oxid68k` hace que el cálculo del test de memoria falle (falso negativo).

2.  **Excepciones de Bus (Bus Error)**:
    *   **Implementado y Verificado**: Se implementó lógica estricta de Bus Error (para accesos fuera de rango y escrituras en ROM).
    *   **Resultado**: El emulador **NO** dispara excepciones de Bus Error durante el arranque. Esto descarta que la ROM dependa de fallos de hardware para detectar memoria. El fallo es puramente lógico o de datos.

3.  **Timing Crítico**:
    *   Interacción crítica entre Video DMA y CPU access timing. Descartado parcialmente ya que el bucle de test corre por 18 segundos antes de fallar.

## Estado Final de la Sesión
El emulador implementa correctamente `Bus Error` (Vector 2), pero esto no resolvió el `Sad Mac 03FFFF`. El análisis del crash apunta fuertemente a defectos en el núcleo CPU (`oxid68k`), específicamente en instrucciones aritméticas/lógicas usadas por el test de memoria, exacerbado por la ausencia total de tests de regresión en el core. Se recomienda validar `oxid68k`.
