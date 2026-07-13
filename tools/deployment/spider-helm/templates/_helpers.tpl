{{/*
Expands the name of the chart.

@return {string} The chart name (truncated to 63 characters)
*/}}
{{- define "spider.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Creates a default fully qualified app name (truncated to 63 chars for the DNS naming spec). If the
release name already contains the chart name it is used as-is.

@return {string} The fully qualified app name (truncated to 63 characters)
*/}}
{{- define "spider.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Creates chart name and version as used by the chart label.

@return {string} Chart name and version (truncated to 63 characters)
*/}}
{{- define "spider.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Creates common labels for all resources.

@return {string} YAML-formatted common labels
*/}}
{{- define "spider.labels" -}}
helm.sh/chart: {{ include "spider.chart" . }}
{{ include "spider.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Creates selector labels for matching resources.

@return {string} YAML-formatted selector labels
*/}}
{{- define "spider.selectorLabels" -}}
app.kubernetes.io/name: {{ include "spider.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Creates a container image reference from `.Values.image.<component>`.

The tag defaults to the chart's appVersion when the component omits it.

@param {object} root Root template context (required)
@param {string} component Key under `.Values.image` (e.g. "storage", "mariadb")
@return {string} Full image reference (repository:tag)
*/}}
{{- define "spider.imageRef" -}}
{{- $img := index .root.Values.image .component -}}
{{- $tag := $img.tag | default .root.Chart.AppVersion -}}
{{- printf "%s:%s" $img.repository $tag -}}
{{- end }}

{{/*
Creates timings for readiness probes (faster checks for quicker startup).

@return {string} YAML-formatted readiness probe timing configuration
*/}}
{{- define "spider.readinessProbeTimings" -}}
initialDelaySeconds: 6
periodSeconds: 2
timeoutSeconds: 2
failureThreshold: 10
{{- end }}

{{/*
Creates timings for liveness probes.

@return {string} YAML-formatted liveness probe timing configuration
*/}}
{{- define "spider.livenessProbeTimings" -}}
initialDelaySeconds: 180
periodSeconds: 30
timeoutSeconds: 4
failureThreshold: 3
{{- end }}

{{/*
Gets the database host. When the bundled MariaDB subchart is enabled, this is its primary Service
(`<release>-mariadb`); otherwise it is the external `spiderConfig.database.host`.

@param {object} . Root template context
@return {string} The database host
*/}}
{{- define "spider.databaseHost" -}}
{{- if .Values.mariadb.enabled -}}
{{- printf "%s-mariadb" .Release.Name -}}
{{- else -}}
{{- .Values.spiderConfig.database.host -}}
{{- end -}}
{{- end }}

{{/*
Gets the database port. When the bundled MariaDB subchart is enabled, this is 3306; otherwise it is
the external `spiderConfig.database.port`.

@param {object} . Root template context
@return {string} The database port
*/}}
{{- define "spider.databasePort" -}}
{{- if .Values.mariadb.enabled -}}
3306
{{- else -}}
{{- .Values.spiderConfig.database.port -}}
{{- end -}}
{{- end }}

{{/*
Gets the database name. From the subchart's `mariadb.userDatabase.name.value` when bundled, else from
`spiderConfig.database.name`.

@param {object} . Root template context
@return {string} The database name
*/}}
{{- define "spider.databaseName" -}}
{{- if .Values.mariadb.enabled -}}
{{- .Values.mariadb.userDatabase.name.value -}}
{{- else -}}
{{- .Values.spiderConfig.database.name -}}
{{- end -}}
{{- end }}

{{/*
Gets the database username. From the subchart's `mariadb.userDatabase.user.value` when bundled, else
from `spiderConfig.database.username`.

@param {object} . Root template context
@return {string} The database username
*/}}
{{- define "spider.databaseUser" -}}
{{- if .Values.mariadb.enabled -}}
{{- .Values.mariadb.userDatabase.user.value -}}
{{- else -}}
{{- .Values.spiderConfig.database.username -}}
{{- end -}}
{{- end }}

{{/*
Gets the database password. From the subchart's `mariadb.userDatabase.password.value` when bundled,
else from `spiderConfig.database.password`.

@param {object} . Root template context
@return {string} The database password
*/}}
{{- define "spider.databasePassword" -}}
{{- if .Values.mariadb.enabled -}}
{{- .Values.mariadb.userDatabase.password.value -}}
{{- else -}}
{{- .Values.spiderConfig.database.password -}}
{{- end -}}
{{- end }}
