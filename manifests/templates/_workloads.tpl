{{- define "og.buildJob" -}}
{{- $id := include "og.indexId" . }}
apiVersion: batch/v1
kind: Job
metadata:
  # A Job's pod template can't change: any change to the build settings is a new Job, which the
  # leader starts in place of the old one.
  name: {{ include "og.name" . }}-build-{{ printf "%s %s %s" $id (include "og.image" .) (toJson (omit .Values.build "retryDelaySeconds")) | sha256sum | trunc 10 }}
  labels:
    {{- include "og.labels" . | nindent 4 }}
    app.kubernetes.io/component: build
  annotations:
    opengeocoding.io/index: {{ $id | quote }}
spec:
  # The leader replaces a failed Job after build.retryDelaySeconds.
  backoffLimit: 0
  activeDeadlineSeconds: {{ .Values.build.activeDeadlineSeconds }}
  template:
    metadata:
      labels:
        {{- include "og.labels" . | nindent 8 }}
        app.kubernetes.io/component: build
      {{- with .Values.build.podAnnotations }}
      annotations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
    spec:
      restartPolicy: Never
      serviceAccountName: {{ include "og.serviceAccount" . }}
      {{- with .Values.imagePullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      securityContext:
        {{- include "og.podSecurity" . | nindent 8 }}
      containers:
        - name: build
          image: {{ include "og.image" . }}
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          command:
            - build_index
            - job
            - --store
            - {{ required "store.url is required" .Values.store.url | quote }}
            - --index
            - {{ $id | quote }}
            - --builder-hash
            - {{ .Values.index.builderHash | quote }}
            - --sources
            - /config/sources.json
            - --scratch
            - /scratch
            - --models
            - /data/libpostal
            - --models-version
            - {{ .Values.index.modelsVersion | quote }}
          env:
            {{- include "og.storeEnv" . | nindent 12 }}
          {{- with .Values.store.credentialsSecret }}
          envFrom:
            - secretRef:
                name: {{ . }}
          {{- end }}
          resources:
            {{- toYaml .Values.build.resources | nindent 12 }}
          securityContext:
            {{- include "og.containerSecurity" . | nindent 12 }}
            readOnlyRootFilesystem: true
          volumeMounts:
            - name: config
              mountPath: /config
              readOnly: true
            - name: scratch
              mountPath: /scratch
            # libpostal reads its models only from its compiled-in /data/libpostal; mounting the
            # scratch root there keeps them on the scratch volume across builds.
            - name: scratch
              mountPath: /data
            - name: tmp
              mountPath: /tmp
      volumes:
        - name: config
          configMap:
            name: {{ include "og.name" . }}-sources
        - name: tmp
          emptyDir: {}
        - name: scratch
          {{- if .Values.build.scratch.existingClaim }}
          persistentVolumeClaim:
            claimName: {{ .Values.build.scratch.existingClaim | quote }}
          {{- else if .Values.build.scratch.emptyDir }}
          emptyDir:
            sizeLimit: {{ .Values.build.scratch.size }}
          {{- else }}
          ephemeral:
            volumeClaimTemplate:
              spec:
                accessModes: [ReadWriteOnce]
                {{- with .Values.build.scratch.storageClassName }}
                storageClassName: {{ . }}
                {{- end }}
                resources:
                  requests:
                    storage: {{ .Values.build.scratch.size }}
          {{- end }}
      {{- with .Values.build.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.build.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.build.affinity }}
      affinity:
        {{- toYaml . | nindent 8 }}
      {{- end }}
{{- end }}

{{- define "og.serving" -}}
{{- $id := include "og.indexId" . }}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "og.name" . }}
  labels:
    {{- include "og.labels" . | nindent 4 }}
    app.kubernetes.io/component: serving
spec:
  {{- if not .Values.serving.autoscaling.enabled }}
  replicas: {{ .Values.serving.replicas }}
  {{- end }}
  progressDeadlineSeconds: {{ .Values.serving.progressDeadlineSeconds }}
  # New pods wait for their index before old ones go, so a rebuild never interrupts serving.
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app.kubernetes.io/instance: {{ .Release.Name }}
      app.kubernetes.io/component: serving
  template:
    metadata:
      labels:
        {{- include "og.labels" . | nindent 8 }}
        app.kubernetes.io/component: serving
      annotations:
        opengeocoding.io/index: {{ $id | quote }}
        {{- with .Values.serving.podAnnotations }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
    spec:
      serviceAccountName: {{ include "og.serviceAccount" . }}
      terminationGracePeriodSeconds: 40
      {{- with .Values.imagePullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      securityContext:
        {{- include "og.podSecurity" . | nindent 8 }}
      initContainers:
        - name: pull-index
          image: {{ include "og.image" . }}
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          command:
            - build_index
            - pull
            - --store
            - {{ required "store.url is required" .Values.store.url | quote }}
            - --index
            - {{ $id | quote }}
            - --to
            - /index
            - --models
            - /data/libpostal
            - --models-version
            - {{ .Values.index.modelsVersion | quote }}
          env:
            {{- include "og.storeEnv" . | nindent 12 }}
          {{- with (.Values.store.readCredentialsSecret | default .Values.store.credentialsSecret) }}
          envFrom:
            - secretRef:
                name: {{ . }}
          {{- end }}
          securityContext:
            {{- include "og.containerSecurity" . | nindent 12 }}
            readOnlyRootFilesystem: true
          volumeMounts:
            - name: index
              mountPath: /index
            - name: data
              mountPath: /data
      containers:
        - name: opengeocoding
          image: {{ include "og.image" . }}
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          env:
            - name: INDEX_DIR
              value: /index
            - name: RPC_PORT
              value: {{ .Values.service.port | quote }}
            - name: METRICS_PORT
              value: {{ .Values.service.metricsPort | quote }}
            {{- with .Values.serving.env }}
            {{- toYaml . | nindent 12 }}
            {{- end }}
          ports:
            - name: rpc
              containerPort: {{ .Values.service.port }}
            - name: metrics
              containerPort: {{ .Values.service.metricsPort }}
          lifecycle:
            # Keeps serving while the Service drops the pod, which happens after SIGTERM.
            preStop:
              exec:
                command: [sleep, "10"]
          startupProbe:
            httpGet:
              path: /healthz
              port: rpc
            periodSeconds: 5
            failureThreshold: 60
          readinessProbe:
            httpGet:
              path: /healthz
              port: rpc
            periodSeconds: 10
            timeoutSeconds: 5
          livenessProbe:
            httpGet:
              path: /healthz
              port: rpc
            periodSeconds: 10
            timeoutSeconds: 15
            failureThreshold: 6
          resources:
            {{- toYaml .Values.serving.resources | nindent 12 }}
          securityContext:
            {{- include "og.containerSecurity" . | nindent 12 }}
          volumeMounts:
            - name: index
              mountPath: /index
              readOnly: true
            - name: data
              mountPath: /data
              readOnly: true
      volumes:
        - name: index
          emptyDir:
            sizeLimit: {{ .Values.serving.indexVolumeSize }}
        - name: data
          emptyDir:
            sizeLimit: 4Gi
      {{- with .Values.serving.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.serving.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.serving.affinity }}
      affinity:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.serving.topologySpreadConstraints }}
      topologySpreadConstraints:
        {{- toYaml . | nindent 8 }}
      {{- end }}
{{- end }}
