# OpsDeck

<p align="center">
  <strong>Centro de control local para supervisar proyectos, detectar riesgos y decidir si están listos para deploy.</strong>
</p>

<p align="center">
  Aplicación de escritorio y herramienta de terminal para macOS.
</p>

---

## ¿Qué es OpsDeck?

OpsDeck reúne en un solo lugar la información más importante de tus proyectos de desarrollo.

Analiza el estado de cada repositorio, revisa la disponibilidad de sus servicios, conserva un historial de monitoreo y genera una decisión clara antes de realizar un deploy.

Todo el análisis se ejecuta localmente en tu computadora, sin depender de servicios de inteligencia artificial externos.

---

## ¿Qué puede hacer?

### Supervisar varios proyectos

Permite registrar diferentes repositorios y consultarlos desde una sola aplicación.

De cada proyecto muestra:

- Ruta local.
- Rama activa.
- Último commit.
- Repositorio remoto.
- Estado del upstream.
- Commits pendientes de subir.
- Commits pendientes de descargar.
- Archivos modificados, preparados o nuevos.

---

### Comprobar la disponibilidad de servicios

Cada proyecto puede tener una URL de health configurada.

OpsDeck verifica:

- Disponibilidad del endpoint.
- Código HTTP.
- Tiempo de respuesta.
- Tipo de contenido.
- Validez de respuestas JSON.
- Errores de conexión.
- Degradación o falta de disponibilidad.

---

### Realizar diagnósticos inteligentes

OpsDeck analiza automáticamente el estado del proyecto mediante reglas locales.

El diagnóstico incluye:

- Puntuación de 0 a 100.
- Nivel de riesgo.
- Problemas detectados.
- Explicación de cada hallazgo.
- Acción recomendada.
- Penalización aplicada a la puntuación.

También permite marcar las recomendaciones como útiles o no útiles para adaptar los análisis futuros de cada proyecto.

---

### Conservar un historial de monitoreo

Cada revisión se guarda localmente para observar cómo cambia el proyecto con el tiempo.

El historial permite comparar:

- Puntuaciones anteriores.
- Estado de salud del servicio.
- Latencia.
- Cantidad de cambios locales.
- Riesgos detectados.
- Evolución general del proyecto.

---

### Detectar anomalías

OpsDeck compara las revisiones recientes y detecta comportamientos fuera de lo normal, como:

- Caídas repentinas en la puntuación.
- Incrementos importantes de latencia.
- Fallos consecutivos del health check.
- Aumento inesperado de cambios locales.
- Divergencia entre ramas.
- Commits remotos pendientes.
- Aparición de posibles archivos sensibles.

---

### Evaluar una lista previa al deploy

Antes de recomendar un deploy, OpsDeck revisa diferentes requisitos:

- Existencia de historial de commits.
- Configuración del repositorio remoto.
- Configuración del upstream.
- Sincronización con la rama remota.
- Estado del árbol de trabajo.
- Posibles archivos sensibles.
- Disponibilidad del servicio.
- Puntuación del diagnóstico.
- Anomalías recientes.

Cada requisito puede aparecer como:

- **Aprobado**
- **Advertencia**
- **Bloqueado**

---

### Aplicar políticas de deploy

Cada proyecto puede usar una política diferente.

#### Desarrollo

Pensada para proyectos que todavía están en construcción.

Permite trabajar con cambios locales y condiciones menos estrictas.

#### Equilibrada

Configuración predeterminada.

Bloquea problemas importantes, pero permite algunas advertencias controladas.

#### Producción

Pensada para proyectos que están por publicarse.

Puede exigir:

- Puntuación mínima alta.
- Health check obligatorio.
- Árbol de trabajo limpio.
- Cero advertencias.
- Latencia máxima.
- Todos los commits respaldados en el remoto.

Las políticas también pueden personalizarse desde la aplicación de escritorio.

---

### Usar un Deploy Gate

El Deploy Gate convierte todos los análisis en una decisión final:

- **Aprobado**
- **Aprobado con advertencias**
- **Bloqueado**

Cuando el proyecto está bloqueado, OpsDeck muestra exactamente qué requisitos deben corregirse.

OpsDeck no realiza el deploy automáticamente; determina si el proyecto está en condiciones adecuadas para hacerlo.

---

### Exportar evidencia

OpsDeck puede generar dos tipos de archivos:

#### Informe Markdown

Incluye:

- Estado del repositorio.
- Health check.
- Puntuación.
- Riesgos.
- Recomendaciones.
- Anomalías.
- Historial reciente.
- Cambios locales.
- Decisión de deploy.

#### Manifiesto JSON

Contiene la evaluación del Deploy Gate en un formato estructurado que puede utilizarse como evidencia o integrarse con otros procesos.

---

## Aplicación de escritorio

La interfaz gráfica permite:

- Consultar todos los proyectos registrados.
- Ejecutar revisiones manuales.
- Activar monitoreo automático.
- Configurar el intervalo de revisión.
- Abrir proyectos en Visual Studio Code.
- Abrir las carpetas de los repositorios.
- Editar políticas de deploy.
- Consultar el historial.
- Revisar anomalías.
- Exportar informes.
- Exportar manifiestos.
- Administrar proyectos.

---

## Herramienta de terminal

OpsDeck también incluye una CLI para consultar rápidamente:

- Estado de proyectos.
- Health checks.
- Diagnósticos.
- Checklist previa al deploy.
- Políticas.
- Deploy Gate.
- Informes.
- Proyectos registrados.

Esto permite utilizar OpsDeck tanto de manera visual como dentro de flujos de trabajo automatizados.

---

## Privacidad

OpsDeck funciona localmente.

La información de los proyectos, políticas, historial y retroalimentación se guarda en la computadora del usuario.

No necesita enviar el código fuente a servicios externos para realizar sus análisis.

---

## Datos locales

OpsDeck conserva su información dentro de:

```text
~/.opsdeck
