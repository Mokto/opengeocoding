{{- define "og.name" -}}
{{- .Release.Name | trunc 40 | trimSuffix "-" -}}
{{- end }}

{{- define "og.labels" -}}
app.kubernetes.io/name: opengeocoding
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 }}
{{- end }}

{{- define "og.image" -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end }}

{{- define "og.sources" -}}
{{ toJson .Values.index.sources }}
{{- end }}

{{/* Names the index by everything that decides its contents. */}}
{{- define "og.indexId" -}}
{{- $s := .Values.index.sources -}}
{{- $digest := printf "%s %s %s" .Values.index.builderHash .Values.index.modelsVersion (include "og.sources" .) | sha256sum | trunc 12 -}}
{{- printf "%s-%s-%s" $s.overtureRelease $s.snapshot $digest -}}
{{- end }}

{{- define "og.serviceAccount" -}}
{{- if .Values.serviceAccount.create -}}
{{ .Values.serviceAccount.name | default (include "og.name" .) }}
{{- else -}}
{{ .Values.serviceAccount.name | default "default" }}
{{- end -}}
{{- end }}

{{- define "og.storeEnv" -}}
- name: AWS_DEFAULT_REGION
  value: {{ .Values.store.region | quote }}
{{- with .Values.store.endpoint }}
- name: AWS_ENDPOINT
  value: {{ . | quote }}
{{- end }}
- name: AWS_ALLOW_HTTP
  value: {{ .Values.store.allowHttp | quote }}
- name: AWS_VIRTUAL_HOSTED_STYLE_REQUEST
  value: {{ .Values.store.virtualHostedStyle | quote }}
{{- with .Values.store.extraEnv }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{- define "og.podSecurity" -}}
runAsNonRoot: true
runAsUser: 1000
runAsGroup: 1000
fsGroup: 1000
seccompProfile:
  type: RuntimeDefault
{{- end }}

{{- define "og.containerSecurity" -}}
allowPrivilegeEscalation: false
capabilities:
  drop: [ALL]
{{- end }}
